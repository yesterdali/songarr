//! Spotify-Connect-style remote control, multi-user. The bot in a voice channel
//! is a **shared room**: everyone currently in that channel controls one queue
//! together. Exclusivity is per server (one room per guild — Discord allows one
//! voice connection per guild); if the bot is busy in another channel of your
//! server you get "busy". A watchdog frees the bot when the channel empties or
//! after an idle timeout. The bot has no inbound HTTP, so Songarr is the relay.
//! See `discord-remote-multiuser-plan.md`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, GuildId, UserId};
use songbird::input::HttpRequest;
use tokio::sync::Mutex;

use crate::playback::set_music_bitrate;
use crate::songarr::{SongarrClient, Track};

const COMMAND_WAIT_SECS: u64 = 20;
const STATE_TICK_SECS: u64 = 5;
const WATCHDOG_TICK_SECS: u64 = 15;
/// Leave voice after this long not actively playing.
const REMOTE_IDLE_SECS: u64 = 120;
const WAVE_REFILL_THRESHOLD: usize = 2;

#[derive(Clone)]
struct TrackMeta {
    id: String,
    title: String,
    artist: String,
    album: Option<String>,
    cover_art: Option<String>,
    duration_ms: Option<i64>,
}

impl TrackMeta {
    fn from_json(value: &serde_json::Value) -> Option<TrackMeta> {
        let id = value.get("id")?.as_str()?.to_string();
        let str_field = |key: &str| value.get(key).and_then(|v| v.as_str()).map(str::to_string);
        Some(TrackMeta {
            id,
            title: str_field("title").unwrap_or_else(|| "Unknown".into()),
            artist: str_field("artist").unwrap_or_else(|| "Unknown artist".into()),
            album: str_field("album"),
            cover_art: str_field("coverArt"),
            duration_ms: value
                .get("duration")
                .and_then(|v| v.as_f64())
                .map(|secs| (secs * 1000.0) as i64),
        })
    }

    fn from_track(track: Track) -> TrackMeta {
        TrackMeta {
            id: track.id,
            title: track.title,
            artist: track.artist,
            album: None,
            cover_art: None,
            duration_ms: None,
        }
    }

    fn track(&self) -> Track {
        Track {
            id: self.id.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
        }
    }
}

/// Shared playback for one guild's voice connection.
struct Room {
    channel_id: ChannelId,
    metas: HashMap<String, TrackMeta>, // songbird uuid → meta
    tracks: Vec<TrackMeta>,            // ordered list (for prev)
    wave: bool,
    not_playing_since: Option<Instant>, // managed by the watchdog
}

impl Room {
    fn new(channel_id: ChannelId) -> Room {
        Room {
            channel_id,
            metas: HashMap::new(),
            tracks: Vec::new(),
            wave: false,
            not_playing_since: None,
        }
    }
}

type Rooms = Arc<Mutex<HashMap<GuildId, Arc<Mutex<Room>>>>>;

enum Status {
    /// In the active room for this guild — may control it.
    Controlling(GuildId),
    /// The bot is in another channel of this guild.
    Busy,
    /// Not in a voice channel / no room.
    Idle,
}

/// Supervisor: shared rooms + a watchdog + one long-poll task per linked user.
pub async fn run(
    ctx: serenity::Context,
    db: sqlx::SqlitePool,
    http: reqwest::Client,
    voice_bitrate_kbps: u32,
) {
    tracing::info!("remote-control supervisor started");
    let rooms: Rooms = Arc::new(Mutex::new(HashMap::new()));
    tokio::spawn(watchdog(ctx.clone(), rooms.clone()));

    let mut spawned: std::collections::HashSet<String> = std::collections::HashSet::new();
    loop {
        if let Ok(links) = crate::store::all_links(&db).await {
            for (discord_id, link) in links {
                if spawned.insert(link.username.clone()) {
                    tokio::spawn(user_loop(
                        ctx.clone(),
                        discord_id,
                        link,
                        http.clone(),
                        rooms.clone(),
                        voice_bitrate_kbps,
                    ));
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

async fn user_loop(
    ctx: serenity::Context,
    discord_id: u64,
    link: crate::store::Link,
    http: reqwest::Client,
    rooms: Rooms,
    voice_bitrate_kbps: u32,
) {
    let client = SongarrClient::new(&http, &link);
    let mut last_seq: i64 = 0;
    let mut active = false;
    loop {
        tokio::select! {
            result = client.remote_commands(last_seq, COMMAND_WAIT_SECS) => {
                match result {
                    Ok(commands) => {
                        for command in &commands {
                            handle_command(&ctx, &client, &http, &rooms, discord_id,
                                &mut active, command, voice_bitrate_kbps).await;
                            last_seq = command.seq.max(last_seq);
                        }
                        if active && !commands.is_empty() {
                            report_state(&ctx, &client, &http, &rooms, discord_id, voice_bitrate_kbps).await;
                        }
                    }
                    Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(STATE_TICK_SECS)), if active => {
                report_state(&ctx, &client, &http, &rooms, discord_id, voice_bitrate_kbps).await;
            }
        }
    }
}

async fn handle_command(
    ctx: &serenity::Context,
    client: &SongarrClient<'_>,
    http: &reqwest::Client,
    rooms: &Rooms,
    discord_id: u64,
    active: &mut bool,
    command: &crate::songarr::RemoteCommand,
    voice_bitrate_kbps: u32,
) {
    match command.action.as_str() {
        "connect" => {
            *active = true;
            let _ = resolve(ctx, rooms, discord_id, true, voice_bitrate_kbps).await;
        }
        "disconnect" => {
            *active = false;
            // Don't leave the room — others may still be listening. Just stop
            // reporting/controlling from this app.
            let _ = client
                .remote_report_state(&serde_json::json!({ "connected": false, "isPlaying": false }))
                .await;
        }
        "play" => {
            if let Status::Controlling(guild) =
                resolve(ctx, rooms, discord_id, true, voice_bitrate_kbps).await
            {
                let tracks = command
                    .payload
                    .get("tracks")
                    .and_then(|v| v.as_array())
                    .map(|items| items.iter().filter_map(TrackMeta::from_json).collect())
                    .unwrap_or_default();
                let start = command
                    .payload
                    .get("startIndex")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                play_into_room(ctx, http, client, rooms, guild, tracks, start, false).await;
            }
        }
        "wave" => {
            if let Status::Controlling(guild) =
                resolve(ctx, rooms, discord_id, true, voice_bitrate_kbps).await
            {
                let tracks: Vec<TrackMeta> = client
                    .wave_next(None, 12)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(TrackMeta::from_track)
                    .collect();
                play_into_room(ctx, http, client, rooms, guild, tracks, 0, true).await;
            }
        }
        "pause" | "resume" | "next" | "seek" | "prev" => {
            if let Status::Controlling(guild) =
                resolve(ctx, rooms, discord_id, false, voice_bitrate_kbps).await
            {
                control(ctx, http, client, rooms, guild, command).await;
            }
        }
        other => tracing::debug!(action = other, "remote: ignoring unknown command"),
    }
}

/// Where does this user stand relative to the bot? Optionally claim an idle bot.
async fn resolve(
    ctx: &serenity::Context,
    rooms: &Rooms,
    discord_id: u64,
    claim: bool,
    voice_bitrate_kbps: u32,
) -> Status {
    let Some((guild, channel)) = user_voice(ctx, UserId::new(discord_id)) else {
        return Status::Idle;
    };
    if let Some(room) = room_arc(rooms, guild).await {
        let in_channel = room.lock().await.channel_id == channel;
        return if in_channel {
            Status::Controlling(guild)
        } else {
            Status::Busy
        };
    }
    if !claim {
        return Status::Idle;
    }
    // Claim the empty slot atomically before the (slow) join.
    {
        let mut map = rooms.lock().await;
        if let Some(room) = map.get(&guild).cloned() {
            drop(map);
            let in_channel = room.lock().await.channel_id == channel;
            return if in_channel {
                Status::Controlling(guild)
            } else {
                Status::Busy
            };
        }
        map.insert(guild, Arc::new(Mutex::new(Room::new(channel))));
    }
    let joined = match songbird::get(ctx).await {
        Some(manager) => match manager.join(guild, channel).await {
            Ok(call) => {
                set_music_bitrate(&call, voice_bitrate_kbps).await;
                true
            }
            Err(_) => false,
        },
        None => false,
    };
    if joined {
        Status::Controlling(guild)
    } else {
        rooms.lock().await.remove(&guild); // roll back the placeholder
        Status::Idle
    }
}

async fn play_into_room(
    ctx: &serenity::Context,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    rooms: &Rooms,
    guild: GuildId,
    tracks: Vec<TrackMeta>,
    start: usize,
    wave: bool,
) {
    let (Some(room), Some(call)) = (room_arc(rooms, guild).await, current_call(ctx, guild).await)
    else {
        return;
    };
    call.lock().await.queue().stop();
    let mut room = room.lock().await;
    room.metas.clear();
    room.tracks = tracks.clone();
    room.wave = wave;
    room.not_playing_since = None;
    for meta in tracks.iter().skip(start) {
        if let Some(uuid) = enqueue_meta(&call, http, client, meta).await {
            room.metas.insert(uuid, meta.clone());
        }
    }
}

async fn control(
    ctx: &serenity::Context,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    rooms: &Rooms,
    guild: GuildId,
    command: &crate::songarr::RemoteCommand,
) {
    let Some(call) = current_call(ctx, guild).await else {
        return;
    };
    match command.action.as_str() {
        "pause" => {
            let _ = call.lock().await.queue().pause();
        }
        "resume" => {
            let _ = call.lock().await.queue().resume();
        }
        "next" => {
            let _ = call.lock().await.queue().skip();
        }
        "seek" => {
            if let Some(ms) = command.payload.get("positionMs").and_then(|v| v.as_i64()) {
                if let Some(handle) = call.lock().await.queue().current() {
                    let _ = handle.seek(Duration::from_millis(ms.max(0) as u64));
                }
            }
        }
        "prev" => prev(ctx, http, client, rooms, guild, &call).await,
        _ => {}
    }
}

/// `prev`: restart if >3s in, else step back through the room's ordered list.
async fn prev(
    ctx: &serenity::Context,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    rooms: &Rooms,
    guild: GuildId,
    call: &Arc<Mutex<songbird::Call>>,
) {
    let Some(handle) = call.lock().await.queue().current() else {
        return;
    };
    let position = handle
        .get_info()
        .await
        .map(|info| info.position)
        .unwrap_or_default();
    if position > Duration::from_secs(3) {
        let _ = handle.seek(Duration::ZERO);
        return;
    }
    let Some(room) = room_arc(rooms, guild).await else {
        return;
    };
    let (tracks, wave, target) = {
        let room = room.lock().await;
        let current_id = room
            .metas
            .get(&handle.uuid().to_string())
            .map(|m| m.id.clone());
        let target = current_id
            .as_ref()
            .and_then(|id| room.tracks.iter().position(|t| &t.id == id))
            .filter(|&i| i > 0)
            .map(|i| i - 1);
        (room.tracks.clone(), room.wave, target)
    };
    match target {
        Some(index) => {
            play_into_room(ctx, http, client, rooms, guild, tracks, index, wave).await;
        }
        None => {
            let _ = handle.seek(Duration::ZERO);
        }
    }
}

async fn report_state(
    ctx: &serenity::Context,
    client: &SongarrClient<'_>,
    http: &reqwest::Client,
    rooms: &Rooms,
    discord_id: u64,
    voice_bitrate_kbps: u32,
) {
    match resolve(ctx, rooms, discord_id, false, voice_bitrate_kbps).await {
        Status::Busy => {
            let _ = client
                .remote_report_state(
                    &serde_json::json!({ "connected": false, "isPlaying": false, "busy": true }),
                )
                .await;
        }
        Status::Idle => {
            let _ = client
                .remote_report_state(&serde_json::json!({ "connected": false, "isPlaying": false }))
                .await;
        }
        Status::Controlling(guild) => {
            // Endless recs: any controller tops up a low queue.
            refill_if_low(ctx, http, client, rooms, guild).await;
            let state = room_state_json(ctx, rooms, guild).await;
            let _ = client.remote_report_state(&state).await;
        }
    }
}

async fn room_state_json(
    ctx: &serenity::Context,
    rooms: &Rooms,
    guild: GuildId,
) -> serde_json::Value {
    let (Some(room), Some(call)) = (room_arc(rooms, guild).await, current_call(ctx, guild).await)
    else {
        return serde_json::json!({ "connected": false, "isPlaying": false });
    };
    let (current, queue) = {
        let handler = call.lock().await;
        (handler.queue().current(), handler.queue().current_queue())
    };
    let room = room.lock().await;
    let mut state = serde_json::json!({ "connected": true, "isPlaying": false });
    if let Some(handle) = &current {
        if let Some(meta) = room.metas.get(&handle.uuid().to_string()) {
            state["trackId"] = serde_json::json!(meta.id);
            state["title"] = serde_json::json!(meta.title);
            state["artist"] = serde_json::json!(meta.artist);
            state["album"] = serde_json::json!(meta.album);
            state["coverArt"] = serde_json::json!(meta.cover_art);
            state["durationMs"] = serde_json::json!(meta.duration_ms);
        }
        if let Ok(info) = handle.get_info().await {
            state["positionMs"] = serde_json::json!(info.position.as_millis() as i64);
            state["isPlaying"] =
                serde_json::json!(matches!(info.playing, songbird::tracks::PlayMode::Play));
        }
    }
    let upcoming: Vec<_> = queue
        .iter()
        .skip(1)
        .filter_map(|handle| room.metas.get(&handle.uuid().to_string()))
        .map(|meta| {
            serde_json::json!({
                "id": meta.id, "title": meta.title, "artist": meta.artist,
                "album": meta.album, "coverArt": meta.cover_art,
            })
        })
        .collect();
    state["queue"] = serde_json::json!(upcoming);
    state
}

async fn refill_if_low(
    ctx: &serenity::Context,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    rooms: &Rooms,
    guild: GuildId,
) {
    let (Some(room), Some(call)) = (room_arc(rooms, guild).await, current_call(ctx, guild).await)
    else {
        return;
    };
    let (wave, seed) = {
        let room = room.lock().await;
        (room.wave, room.tracks.last().map(|t| t.id.clone()))
    };
    if !wave || call.lock().await.queue().len() > WAVE_REFILL_THRESHOLD {
        return;
    }
    let fresh = client
        .wave_next(seed.as_deref(), 12)
        .await
        .unwrap_or_default();
    let mut room = room.lock().await;
    for track in fresh {
        let meta = TrackMeta::from_track(track);
        if room.tracks.iter().any(|t| t.id == meta.id) {
            continue;
        }
        if let Some(uuid) = enqueue_meta(&call, http, client, &meta).await {
            room.metas.insert(uuid, meta.clone());
            room.tracks.push(meta);
        }
    }
}

/// Watchdog: free the bot from rooms whose channel is empty or that have been
/// idle past the timeout.
async fn watchdog(ctx: serenity::Context, rooms: Rooms) {
    loop {
        tokio::time::sleep(Duration::from_secs(WATCHDOG_TICK_SECS)).await;
        let entries: Vec<(GuildId, Arc<Mutex<Room>>)> = {
            rooms
                .lock()
                .await
                .iter()
                .map(|(g, r)| (*g, r.clone()))
                .collect()
        };
        for (guild, room) in entries {
            let channel = room.lock().await.channel_id;
            let call = current_call(&ctx, guild).await;
            let Some(call) = call else {
                rooms.lock().await.remove(&guild);
                continue;
            };
            // Empty channel (no humans) → leave.
            if channel_humans(&ctx, guild, channel) == 0 {
                leave(&ctx, &rooms, guild).await;
                continue;
            }
            // Idle too long → leave.
            let playing = match call.lock().await.queue().current() {
                Some(handle) => handle
                    .get_info()
                    .await
                    .map(|i| matches!(i.playing, songbird::tracks::PlayMode::Play))
                    .unwrap_or(false),
                None => false,
            };
            let mut room = room.lock().await;
            if playing {
                room.not_playing_since = None;
            } else {
                let since = *room.not_playing_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(REMOTE_IDLE_SECS) {
                    drop(room);
                    leave(&ctx, &rooms, guild).await;
                }
            }
        }
    }
}

async fn leave(ctx: &serenity::Context, rooms: &Rooms, guild: GuildId) {
    if let Some(manager) = songbird::get(ctx).await {
        let _ = manager.remove(guild).await;
    }
    rooms.lock().await.remove(&guild);
    tracing::info!(?guild, "remote: left voice (empty/idle)");
}

async fn enqueue_meta(
    call: &Arc<Mutex<songbird::Call>>,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    meta: &TrackMeta,
) -> Option<String> {
    let url = client.stream_url(&meta.track());
    let input = HttpRequest::new(http.clone(), url);
    let handle = {
        let mut handler = call.lock().await;
        handler.enqueue_input(input.into()).await
    };
    Some(handle.uuid().to_string())
}

async fn room_arc(rooms: &Rooms, guild: GuildId) -> Option<Arc<Mutex<Room>>> {
    rooms.lock().await.get(&guild).cloned()
}

async fn current_call(
    ctx: &serenity::Context,
    guild: GuildId,
) -> Option<Arc<Mutex<songbird::Call>>> {
    songbird::get(ctx).await?.get(guild)
}

/// The guild/voice-channel a Discord user is currently in (gateway cache).
fn user_voice(ctx: &serenity::Context, user: UserId) -> Option<(GuildId, ChannelId)> {
    for guild_id in ctx.cache.guilds() {
        if let Some(guild) = ctx.cache.guild(guild_id) {
            if let Some(state) = guild.voice_states.get(&user) {
                if let Some(channel_id) = state.channel_id {
                    return Some((guild_id, channel_id));
                }
            }
        }
    }
    None
}

/// Count human (non-bot) members in a voice channel.
fn channel_humans(ctx: &serenity::Context, guild: GuildId, channel: ChannelId) -> usize {
    let bot = ctx.cache.current_user().id;
    ctx.cache
        .guild(guild)
        .map(|g| {
            g.voice_states
                .values()
                .filter(|vs| vs.channel_id == Some(channel) && vs.user_id != bot)
                .count()
        })
        .unwrap_or(0)
}
