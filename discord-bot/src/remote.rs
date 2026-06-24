//! Spotify-Connect-style remote control. One task per linked user **long-polls**
//! that user's command queue via Songarr (so control is near-instant, not
//! polled), drives songbird (join voice, play, pause, skip, prev, seek, endless
//! recs), and reports playback state back to Songarr for the app's playbar. The
//! bot has no inbound HTTP, so Songarr is the relay. See `discord-remote-plan.md`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, GuildId, UserId};
use songbird::input::HttpRequest;
use tokio::sync::Mutex;

use crate::songarr::{SongarrClient, Track};

/// How long the command long-poll blocks server-side (< the client read timeout).
const COMMAND_WAIT_SECS: u64 = 20;
/// Cadence for state reports / wave refills while connected.
const STATE_TICK_SECS: u64 = 5;
/// Refill endless recs when the queue drops to this many tracks.
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
        Track { id: self.id.clone(), title: self.title.clone(), artist: self.artist.clone() }
    }
}

#[derive(Default)]
struct UserSession {
    last_seq: i64,
    guild_id: Option<GuildId>,
    /// songbird handle UUID → metadata (for state reports).
    metas: HashMap<String, TrackMeta>,
    /// Ordered list from the last play, so `prev` can step back.
    tracks: Vec<TrackMeta>,
    /// Endless recs mode: refill the queue when it runs low.
    wave: bool,
}

/// Supervisor: spawn one long-poll task per linked user.
pub async fn run(ctx: serenity::Context, db: sqlx::SqlitePool, http: reqwest::Client) {
    tracing::info!("remote-control supervisor started");
    let mut spawned: HashSet<String> = HashSet::new();
    loop {
        if let Ok(links) = crate::store::all_links(&db).await {
            for (discord_id, link) in links {
                if spawned.insert(link.username.clone()) {
                    tokio::spawn(user_loop(ctx.clone(), discord_id, link, http.clone()));
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
) {
    let client = SongarrClient::new(&http, &link);
    let mut session = UserSession::default();
    loop {
        let connected = session.guild_id.is_some();
        tokio::select! {
            result = client.remote_commands(session.last_seq, COMMAND_WAIT_SECS) => {
                match result {
                    Ok(commands) => {
                        let had = !commands.is_empty();
                        for command in &commands {
                            handle_command(&ctx, &client, &http, discord_id, &mut session, command)
                                .await;
                            session.last_seq = command.seq.max(session.last_seq);
                        }
                        if had && session.guild_id.is_some() {
                            report_state(&ctx, &client, &session).await;
                        }
                    }
                    Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(STATE_TICK_SECS)), if connected => {
                refill_if_low(&ctx, &client, &http, &mut session).await;
                report_state(&ctx, &client, &session).await;
            }
        }
    }
}

async fn handle_command(
    ctx: &serenity::Context,
    client: &SongarrClient<'_>,
    http: &reqwest::Client,
    discord_id: u64,
    session: &mut UserSession,
    command: &crate::songarr::RemoteCommand,
) {
    match command.action.as_str() {
        "connect" => connect(ctx, discord_id, session).await,
        "disconnect" => {
            if let Some(guild_id) = session.guild_id.take() {
                if let Some(manager) = songbird::get(ctx).await {
                    let _ = manager.remove(guild_id).await;
                }
                session.metas.clear();
                session.tracks.clear();
                session.wave = false;
                let _ = client
                    .remote_report_state(
                        &serde_json::json!({ "connected": false, "isPlaying": false }),
                    )
                    .await;
            }
        }
        "play" => {
            if session.guild_id.is_none() {
                connect(ctx, discord_id, session).await;
            }
            let tracks = command
                .payload
                .get("tracks")
                .and_then(|v| v.as_array())
                .map(|items| items.iter().filter_map(TrackMeta::from_json).collect())
                .unwrap_or_default();
            let start = command.payload.get("startIndex").and_then(|v| v.as_u64()).unwrap_or(0)
                as usize;
            play_tracks(ctx, http, client, session, tracks, start, false).await;
        }
        "wave" => {
            if session.guild_id.is_none() {
                connect(ctx, discord_id, session).await;
            }
            let tracks: Vec<TrackMeta> = client
                .wave_next(None, 12)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(TrackMeta::from_track)
                .collect();
            play_tracks(ctx, http, client, session, tracks, 0, true).await;
        }
        "pause" => {
            if let Some(call) = current_call(ctx, session).await {
                let _ = call.lock().await.queue().pause();
            }
        }
        "resume" => {
            if let Some(call) = current_call(ctx, session).await {
                let _ = call.lock().await.queue().resume();
            }
        }
        "next" => {
            if let Some(call) = current_call(ctx, session).await {
                let _ = call.lock().await.queue().skip();
            }
        }
        "prev" => prev(ctx, http, client, session).await,
        "seek" => {
            if let Some(ms) = command.payload.get("positionMs").and_then(|v| v.as_i64()) {
                if let Some(handle) = current_handle(ctx, session).await {
                    let _ = handle.seek(Duration::from_millis(ms.max(0) as u64));
                }
            }
        }
        other => tracing::debug!(action = other, "remote: ignoring unknown command"),
    }
}

async fn connect(ctx: &serenity::Context, discord_id: u64, session: &mut UserSession) {
    let Some((guild_id, channel_id)) = find_voice_channel(ctx, UserId::new(discord_id)) else {
        tracing::info!(discord_id, "remote connect: user not in a voice channel");
        return;
    };
    let Some(manager) = songbird::get(ctx).await else {
        return;
    };
    match manager.join(guild_id, channel_id).await {
        Ok(_) => session.guild_id = Some(guild_id),
        Err(error) => tracing::warn!(%error, "remote connect: join failed"),
    }
}

async fn play_tracks(
    ctx: &serenity::Context,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    session: &mut UserSession,
    tracks: Vec<TrackMeta>,
    start: usize,
    wave: bool,
) {
    let Some(call) = current_call(ctx, session).await else {
        return;
    };
    call.lock().await.queue().stop(); // clear whatever was playing
    session.metas.clear();
    session.tracks = tracks.clone();
    session.wave = wave;
    for meta in tracks.iter().skip(start) {
        if let Some(uuid) = enqueue_meta(&call, http, client, meta).await {
            session.metas.insert(uuid, meta.clone());
        }
    }
}

/// `prev`: restart the current track if >3s in, else step to the previous track
/// (re-enqueue the ordered list from there). songbird has no native previous.
async fn prev(
    ctx: &serenity::Context,
    http: &reqwest::Client,
    client: &SongarrClient<'_>,
    session: &mut UserSession,
) {
    let Some(handle) = current_handle(ctx, session).await else {
        return;
    };
    let position = handle.get_info().await.map(|info| info.position).unwrap_or_default();
    let current_id = session.metas.get(&handle.uuid().to_string()).map(|m| m.id.clone());
    if position > Duration::from_secs(3) {
        let _ = handle.seek(Duration::ZERO);
        return;
    }
    let target = current_id
        .as_ref()
        .and_then(|id| session.tracks.iter().position(|t| &t.id == id))
        .filter(|&i| i > 0)
        .map(|i| i - 1);
    match target {
        Some(index) => {
            let tracks = session.tracks.clone();
            let wave = session.wave;
            play_tracks(ctx, http, client, session, tracks, index, wave).await;
        }
        None => {
            let _ = handle.seek(Duration::ZERO);
        }
    }
}

/// Endless recs: top up the queue from Wave when it runs low.
async fn refill_if_low(
    ctx: &serenity::Context,
    client: &SongarrClient<'_>,
    http: &reqwest::Client,
    session: &mut UserSession,
) {
    if !session.wave {
        return;
    }
    let Some(call) = current_call(ctx, session).await else {
        return;
    };
    let remaining = call.lock().await.queue().len();
    if remaining > WAVE_REFILL_THRESHOLD {
        return;
    }
    let seed = session.tracks.last().map(|t| t.id.clone());
    let fresh = client.wave_next(seed.as_deref(), 12).await.unwrap_or_default();
    for track in fresh {
        let meta = TrackMeta::from_track(track);
        if session.tracks.iter().any(|t| t.id == meta.id) {
            continue;
        }
        if let Some(uuid) = enqueue_meta(&call, http, client, &meta).await {
            session.metas.insert(uuid, meta.clone());
            session.tracks.push(meta);
        }
    }
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

async fn report_state(ctx: &serenity::Context, client: &SongarrClient<'_>, session: &UserSession) {
    let Some(guild_id) = session.guild_id else {
        return;
    };
    let Some(manager) = songbird::get(ctx).await else {
        return;
    };
    let Some(call) = manager.get(guild_id) else {
        let _ = client
            .remote_report_state(&serde_json::json!({ "connected": false, "isPlaying": false }))
            .await;
        return;
    };

    let (current, queue) = {
        let handler = call.lock().await;
        (handler.queue().current(), handler.queue().current_queue())
    };

    let mut state = serde_json::json!({ "connected": true, "isPlaying": false });
    if let Some(handle) = &current {
        if let Some(meta) = session.metas.get(&handle.uuid().to_string()) {
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
        .filter_map(|handle| session.metas.get(&handle.uuid().to_string()))
        .map(|meta| {
            serde_json::json!({
                "id": meta.id,
                "title": meta.title,
                "artist": meta.artist,
                "album": meta.album,
                "coverArt": meta.cover_art,
            })
        })
        .collect();
    state["queue"] = serde_json::json!(upcoming);

    let _ = client.remote_report_state(&state).await;
}

async fn current_call(
    ctx: &serenity::Context,
    session: &UserSession,
) -> Option<Arc<Mutex<songbird::Call>>> {
    let guild_id = session.guild_id?;
    songbird::get(ctx).await?.get(guild_id)
}

async fn current_handle(
    ctx: &serenity::Context,
    session: &UserSession,
) -> Option<songbird::tracks::TrackHandle> {
    let call = current_call(ctx, session).await?;
    let handle = call.lock().await.queue().current();
    handle
}

/// Find which guild/voice-channel a Discord user is currently in (gateway cache).
fn find_voice_channel(ctx: &serenity::Context, user: UserId) -> Option<(GuildId, ChannelId)> {
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
