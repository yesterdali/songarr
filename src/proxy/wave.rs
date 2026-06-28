//! Songarr Wave PWA serving.
//!
//! Production serves the Vite build embedded from `web/dist` under `/wave/`.
//! `/wave/spike` is a tiny browser-audio probe for Phase 0 device testing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};

use crate::proxy::similar::SeedTrack;
use crate::subsonic::types::SongEntry;
use crate::vtrack;
use crate::AppState;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WaveAssets;

const DEFAULT_NEXT_COUNT: usize = 12;
const MAX_NEXT_COUNT: usize = 30;
const SEED_EXTENSION_TIMEOUT_SECS: u64 = 2;
const FEEDBACK_SKIP_COOLDOWN_DAYS: i64 = 14;
const FEEDBACK_DISLIKE_COOLDOWN_DAYS: i64 = 365;
const MAX_RANDOM_FALLBACK_COUNT: usize = 90;

pub async fn index() -> Response {
    asset_response("index.html", true)
}

pub async fn asset(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');
    if path.is_empty() || path == "spike" {
        return index().await;
    }
    asset_response(path, false)
}

pub async fn spike(Query(params): Query<HashMap<String, String>>) -> Response {
    let id = params.get("id").cloned().unwrap_or_default();
    let auth = auth_query(&params);
    let stream_url = if id.is_empty() || auth.is_empty() {
        String::new()
    } else {
        format!("/rest/stream?{auth}&id={}", percent_encode(&id))
    };
    Html(format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
    <title>Songarr Wave audio spike</title>
    <style>
      :root {{ color-scheme: light dark; font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
      body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: #111114; color: white; }}
      main {{ width: min(92vw, 520px); }}
      button {{ width: 100%; border: 0; border-radius: 18px; padding: 18px 20px; font: inherit; font-weight: 800; background: linear-gradient(120deg,#ff8a3d,#ff4d9d,#7c3aed); color: white; }}
      audio {{ width: 100%; margin-top: 18px; }}
      code {{ overflow-wrap: anywhere; }}
      .muted {{ color: #a1a1aa; }}
    </style>
  </head>
  <body>
    <main>
      <h1>Songarr Wave audio spike</h1>
      <p class="muted">Open this page on the target phone with normal Subsonic auth and a virtual track id:</p>
      <p><code>/wave/spike?u=admin&t=...&s=...&v=1.16.1&c=wave&id=sgr_...</code></p>
      <button id="play" type="button">Play test stream</button>
      <audio id="audio" controls preload="none" src="{stream_url}"></audio>
      <p id="status" class="muted">{status}</p>
    </main>
    <script>
      const audio = document.getElementById("audio");
      const status = document.getElementById("status");
      const play = document.getElementById("play");
      if ("mediaSession" in navigator) {{
        navigator.mediaSession.metadata = new MediaMetadata({{
          title: "Songarr Wave audio spike",
          artist: "Songarr",
          album: "Phase 0"
        }});
        navigator.mediaSession.setActionHandler("play", () => audio.play());
        navigator.mediaSession.setActionHandler("pause", () => audio.pause());
      }}
      play.addEventListener("click", async () => {{
        try {{
          await audio.play();
          status.textContent = "Playing. Lock the phone and check whether audio + lock-screen controls survive.";
        }} catch (error) {{
          status.textContent = "Play failed: " + error;
        }}
      }});
      audio.addEventListener("error", () => {{
        status.textContent = "Audio element error. Check auth, id, and Songarr logs.";
      }});
    </script>
  </body>
</html>"#,
        stream_url = html_escape(&stream_url),
        status = if stream_url.is_empty() {
            "Missing id or auth query params.".to_string()
        } else {
            "Ready. Tap play.".to_string()
        }
    ))
    .into_response()
}

pub async fn next_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, auth_stream, auth_json) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let count = requested_count(params.get("count").map(String::as_str));
    let seed_id = params.get("seedId").filter(|id| !id.is_empty()).cloned();
    let tracks =
        match next_tracks(&state, &username, &auth_stream, &auth_json, seed_id, count).await {
            Ok(tracks) => tracks,
            Err(error) => {
                tracing::warn!(%error, username, "wave next failed");
                Vec::new()
            }
        };
    spawn_lyrics_prefetch(&state, &tracks);
    tracing::info!(username, count = tracks.len(), "served wave next tracks");
    Json(WaveNextResponse { tracks }).into_response()
}

/// Fill the lyrics cache for a fresh batch in the background, so the lyrics
/// button is ready the moment a recommended track starts playing.
fn spawn_lyrics_prefetch(state: &AppState, tracks: &[WaveTrack]) {
    const PREFETCH_COUNT: usize = 6;
    if !state.config.lyrics.enabled || tracks.is_empty() {
        return;
    }
    let songs: Vec<_> = tracks
        .iter()
        .take(PREFETCH_COUNT)
        .map(|track| {
            (
                track.artist.clone(),
                track.title.clone(),
                track.album.clone(),
                track.duration_secs,
            )
        })
        .collect();
    let state = state.clone();
    tokio::spawn(async move {
        for (artist, title, album, duration_secs) in songs {
            // Sequential on purpose: cache misses go out to LRCLIB, stay polite.
            if let Err(error) =
                crate::lyrics::lookup(&state, &artist, &title, album.as_deref(), duration_secs)
                    .await
            {
                tracing::debug!(%error, artist, title, "wave lyrics prefetch failed");
            }
        }
    });
}

pub async fn feedback_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<FeedbackRequest>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let action = body.action.trim();
    if !matches!(action, "play" | "skip" | "like" | "dislike") {
        return (StatusCode::BAD_REQUEST, "unknown feedback action").into_response();
    }
    match record_feedback(&state, &username, &body.track_id, action).await {
        Ok(()) => Json(serde_json::json!({ "status": "ok" })).into_response(),
        Err(error) => {
            tracing::warn!(%error, username, track_id = body.track_id, action, "wave feedback failed");
            (StatusCode::BAD_REQUEST, "feedback failed").into_response()
        }
    }
}

async fn authenticated(
    state: &AppState,
    params: &HashMap<String, String>,
) -> Result<(String, String, String), Response> {
    let username = params.get("u").cloned().unwrap_or_default();
    if username.is_empty() {
        return Err((StatusCode::UNAUTHORIZED, "missing username").into_response());
    }
    let auth_stream = auth_query_with_format(params, None);
    let auth_json = auth_query_with_format(params, Some("json"));
    let url = format!(
        "{}/rest/ping?{auth_json}",
        state.config.navidrome.base_url.trim_end_matches('/')
    );
    let ok = match state.http.get(url).send().await {
        Ok(response) if response.status().is_success() => response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|value| {
                value["subsonic-response"]["status"]
                    .as_str()
                    .map(|status| status == "ok")
            })
            .unwrap_or(false),
        _ => false,
    };
    if ok {
        Ok((username, auth_stream, auth_json))
    } else {
        Err((StatusCode::UNAUTHORIZED, "auth failed").into_response())
    }
}

mod imports;
pub use imports::imports_handler;

mod activity;
use activity::display_names;
pub use activity::{
    avatar_delete_handler, avatar_get_handler, avatar_set_handler, friends_handler,
    now_playing_handler, profile_get_handler, profile_set_handler,
};

mod listen;
pub use listen::{
    listen_command_handler, listen_create_handler, listen_join_handler, listen_leave_handler,
    listen_state_handler, listen_sweeper, listen_time_handler, ListenSessions,
};
#[cfg(test)]
use listen::{push_listen_event, snapshot, ListenSession, ListenTrack};

mod remote;
pub use remote::{
    remote_command_handler, remote_commands_handler, remote_state_handler,
    remote_state_report_handler,
};

/// Ingest a pasted YouTube/Yandex/VK link into a virtual track, returning a
/// streamable `sgr_` id. Auth (Subsonic creds) is in the query string, the link
/// in the JSON body — mirroring `feedback_handler`.
pub async fn ingest_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<IngestRequest>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let url = body.url.trim();
    if url.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing url").into_response();
    }
    match crate::ingest_url::build_from_url(&state, url).await {
        Ok(ingested) => {
            tracing::info!(
                username,
                url,
                id = ingested.id,
                provider = ingested.provider,
                "ingested link"
            );
            Json(IngestResponse {
                id: ingested.id,
                artist: ingested.artist,
                title: ingested.title,
                provider: ingested.provider.to_string(),
            })
            .into_response()
        }
        Err(error) => {
            tracing::warn!(%error, username, url, "link ingest failed");
            // Unsupported/unparseable link → client error; anything else (a
            // failed yt-dlp/Yandex extraction) → upstream error.
            let status = if error.to_string().starts_with("unsupported link") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::BAD_GATEWAY
            };
            (status, format!("{error}")).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestResponse {
    id: String,
    artist: String,
    title: String,
    provider: String,
}

mod recommendations;
use recommendations::{next_tracks, record_feedback, FeedbackRequest, WaveNextResponse, WaveTrack};

fn asset_response(path: &str, exact: bool) -> Response {
    let asset = WaveAssets::get(path).or_else(|| {
        if exact {
            None
        } else {
            WaveAssets::get("index.html")
        }
    });
    let Some(asset) = asset else {
        return (StatusCode::NOT_FOUND, "wave asset not found").into_response();
    };
    let content_type = content_type(path);
    let cache_control = if path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from(asset.data.into_owned()))
        .unwrap();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(cache_control));
    response
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/manifest+json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn auth_query(params: &HashMap<String, String>) -> String {
    auth_query_with_format(params, params.get("f").map(String::as_str))
}

fn auth_query_with_format(params: &HashMap<String, String>, format: Option<&str>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for key in ["u", "t", "s", "p", "v", "c"] {
        if let Some(value) = params.get(key) {
            serializer.append_pair(key, value);
        }
    }
    if let Some(format) = format {
        serializer.append_pair("f", format);
    }
    serializer.finish()
}

fn requested_count(raw: Option<&str>) -> usize {
    raw.and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_NEXT_COUNT)
        .clamp(1, MAX_NEXT_COUNT)
}

fn requested_import_limit(raw: Option<&str>) -> i64 {
    raw.and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 200)
}

fn percent_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_cover_vite_assets() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type("assets/app.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type("assets/app.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("icon-192.png"), "image/png");
    }

    #[test]
    fn spike_auth_query_keeps_only_subsonic_auth() {
        let params = HashMap::from([
            ("u".to_string(), "admin".to_string()),
            ("t".to_string(), "token".to_string()),
            ("s".to_string(), "salt".to_string()),
            ("id".to_string(), "sgr_one".to_string()),
        ]);
        assert_eq!(auth_query(&params), "u=admin&t=token&s=salt");
        assert_eq!(
            auth_query_with_format(&params, Some("json")),
            "u=admin&t=token&s=salt&f=json"
        );
    }

    #[test]
    fn requested_count_is_bounded() {
        assert_eq!(requested_count(None), DEFAULT_NEXT_COUNT);
        assert_eq!(requested_count(Some("0")), 1);
        assert_eq!(requested_count(Some("999")), MAX_NEXT_COUNT);
    }

    #[test]
    fn requested_import_limit_is_bounded() {
        assert_eq!(requested_import_limit(None), 50);
        assert_eq!(requested_import_limit(Some("0")), 1);
        assert_eq!(requested_import_limit(Some("5000")), 200);
    }

    #[test]
    fn listen_track_accepts_web_play_payload() {
        let track: ListenTrack = serde_json::from_value(serde_json::json!({
            "id": "sgr_1",
            "title": "Song",
            "artist": "Artist",
            "provider": "yandex",
            "artistId": "artist-1",
            "album": "Album",
            "albumId": "album-1",
            "coverArt": "cover-1",
            "durationMs": 123_000
        }))
        .expect("durationMs payload");
        assert_eq!(track.provider.as_deref(), Some("yandex"));
        assert_eq!(track.artist_id.as_deref(), Some("artist-1"));
        assert_eq!(track.album_id.as_deref(), Some("album-1"));
        assert_eq!(track.duration_ms, Some(123_000));
    }

    #[test]
    fn listen_snapshot_keeps_events_oldest_first() {
        let mut session = ListenSession {
            code: "ABC123".into(),
            host: "admin".into(),
            members: HashMap::from([("admin".into(), 1)]),
            tracks: Vec::new(),
            events: Vec::new(),
            index: 0,
            anchor_pos_ms: 0,
            anchor_ts_ms: 0,
            is_playing: false,
            rev: 1,
        };
        for i in 0..60 {
            push_listen_event(&mut session, "admin", "chat", &format!("event-{i}"), i);
        }
        let snap = snapshot(&session);
        assert_eq!(snap.events.len(), 50);
        assert_eq!(
            snap.events.first().map(|e| e.text.as_str()),
            Some("event-10")
        );
        assert_eq!(
            snap.events.last().map(|e| e.text.as_str()),
            Some("event-59")
        );
    }
}
