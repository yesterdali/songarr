//! Spotify-Connect-style remote control. The Songarr app writes commands; this
//! background loop (one per process) polls each linked user's command queue via
//! Songarr, drives songbird (join voice, play, pause, skip, seek), and reports
//! playback state back to Songarr for the app's playbar. See
//! `discord-remote-plan.md`. The bot has no inbound HTTP, so Songarr is the relay.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::{ChannelId, GuildId, UserId};
use songbird::input::HttpRequest;
use tokio::sync::Mutex;

use crate::songarr::{RemoteCommand, SongarrClient, Track};

/// Display metadata for a queued track, kept so state reports can name what's
/// playing (songbird only tracks opaque handle UUIDs).
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
}

#[derive(Default)]
struct UserSession {
    last_seq: i64,
    guild_id: Option<GuildId>,
    /// songbird handle UUID → metadata for the tracks we enqueued.
    metas: HashMap<String, TrackMeta>,
}

/// Run the remote-control loop forever. Spawned once at startup.
pub async fn run(
    ctx: serenity::Context,
    db: sqlx::SqlitePool,
    http: reqwest::Client,
    stream_http: reqwest::Client,
) {
    let mut sessions: HashMap<String, UserSession> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    tracing::info!("remote-control loop started");
    loop {
        ticker.tick().await;
        let links = match crate::store::all_links(&db).await {
            Ok(links) => links,
            Err(error) => {
                tracing::debug!(%error, "remote: listing links failed");
                continue;
            }
        };
        for (discord_id, link) in links {
            let client = SongarrClient::new(&http, &link);
            let session = sessions
                .entry(link.username.clone())
                .or_insert_with(UserSession::default);

            let commands = client.remote_commands(session.last_seq).await.unwrap_or_default();
            for command in &commands {
                handle_command(&ctx, &client, &stream_http, discord_id, session, command).await;
                session.last_seq = command.seq.max(session.last_seq);
            }
            if session.guild_id.is_some() {
                report_state(&ctx, &client, session).await;
            }
        }
    }
}

async fn handle_command(
    ctx: &serenity::Context,
    client: &SongarrClient<'_>,
    stream_http: &reqwest::Client,
    discord_id: u64,
    session: &mut UserSession,
    command: &RemoteCommand,
) {
    match command.action.as_str() {
        "connect" => {
            connect(ctx, discord_id, session).await;
        }
        "disconnect" => {
            if let Some(guild_id) = session.guild_id.take() {
                if let Some(manager) = songbird::get(ctx).await {
                    let _ = manager.remove(guild_id).await;
                }
                session.metas.clear();
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
            play(ctx, stream_http, client, session, &command.payload).await;
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
        "prev" => {
            // No native previous in songbird (Phase 2 adds a history stack); for
            // now restart the current track.
            if let Some(handle) = current_handle(ctx, session).await {
                let _ = handle.seek(Duration::from_millis(0));
            }
        }
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

async fn play(
    ctx: &serenity::Context,
    stream_http: &reqwest::Client,
    client: &SongarrClient<'_>,
    session: &mut UserSession,
    payload: &serde_json::Value,
) {
    let Some(call) = current_call(ctx, session).await else {
        return;
    };
    let tracks = payload.get("tracks").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let start = payload.get("startIndex").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    // Replace whatever was playing.
    {
        let handler = call.lock().await;
        handler.queue().stop();
    }
    session.metas.clear();

    for value in tracks.iter().skip(start) {
        let Some(meta) = TrackMeta::from_json(value) else {
            continue;
        };
        let url = client.stream_url(&Track {
            id: meta.id.clone(),
            title: meta.title.clone(),
            artist: meta.artist.clone(),
        });
        let input = HttpRequest::new(stream_http.clone(), url);
        let handle = {
            let mut handler = call.lock().await;
            handler.enqueue_input(input.into()).await
        };
        session.metas.insert(handle.uuid().to_string(), meta);
    }
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

/// Find which guild/voice-channel a Discord user is currently in (from the
/// gateway cache). Returns the first match.
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
