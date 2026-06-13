//! Songarr Wave PWA serving.
//!
//! Production serves the Vite build embedded from `web/dist` under `/wave/`.
//! `/wave/spike` is a tiny browser-audio probe for Phase 0 device testing.

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
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
const SEED_EXTENSION_TIMEOUT_SECS: u64 = 4;
const FEEDBACK_SKIP_COOLDOWN_DAYS: i64 = 14;
const FEEDBACK_DISLIKE_COOLDOWN_DAYS: i64 = 365;

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

#[derive(Debug, Serialize)]
struct WaveNextResponse {
    tracks: Vec<WaveTrack>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WaveTrack {
    id: String,
    title: String,
    artist: String,
    artist_id: Option<String>,
    album: Option<String>,
    album_id: Option<String>,
    duration_secs: Option<i64>,
    cover_art: Option<String>,
    stream_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackRequest {
    track_id: String,
    action: String,
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

async fn next_tracks(
    state: &AppState,
    username: &str,
    auth_stream: &str,
    auth_json: &str,
    seed_id: Option<String>,
    count: usize,
) -> anyhow::Result<Vec<WaveTrack>> {
    let suppression = FeedbackSuppression::load(state, username).await;
    let mut tracks = Vec::new();
    let mut existing = Vec::new();

    if let Some(seed_id) = seed_id {
        if let Ok(seed) = seed_from_id(state, &seed_id).await {
            extend_from_seed(
                state,
                username,
                auth_stream,
                &suppression,
                &mut existing,
                &mut tracks,
                seed,
                count,
            )
            .await;
        }
    } else {
        for seed in positive_feedback_seeds(state, username, 2).await? {
            let remaining = count.saturating_sub(tracks.len());
            if remaining == 0 {
                break;
            }
            extend_from_seed(
                state,
                username,
                auth_stream,
                &suppression,
                &mut existing,
                &mut tracks,
                seed,
                remaining.min(6),
            )
            .await;
        }
        for seed in crate::recs::recent_listen_seeds(&state.db, username, 2).await? {
            let remaining = count.saturating_sub(tracks.len());
            if remaining == 0 {
                break;
            }
            let seed = listen_seed_to_seed_track(state, seed).await;
            extend_from_seed(
                state,
                username,
                auth_stream,
                &suppression,
                &mut existing,
                &mut tracks,
                seed,
                remaining.min(6),
            )
            .await;
        }
    }

    if tracks.len() < count {
        match random_tracks(state, auth_stream, auth_json, count - tracks.len()).await {
            Ok(random) => {
                for track in random {
                    let key = crate::recs::song_key(&track.artist, &track.title);
                    let track_key = crate::proxy::search::SongKey::new(
                        &track.artist,
                        &track.title,
                        track.duration_secs,
                    );
                    if !suppression.is_suppressed(&track.artist, &track.title)
                        && !existing.iter().any(|existing| existing.matches(&track_key))
                    {
                        if !tracks.iter().any(|t| {
                            t.id == track.id || crate::recs::song_key(&t.artist, &t.title) == key
                        }) {
                            existing.push(track_key);
                            tracks.push(track);
                        }
                    }
                    if tracks.len() >= count {
                        break;
                    }
                }
            }
            Err(error) => tracing::debug!(%error, "wave random fallback failed"),
        }
    }

    Ok(tracks)
}

async fn extend_from_seed(
    state: &AppState,
    username: &str,
    auth_stream: &str,
    suppression: &FeedbackSuppression,
    existing: &mut Vec<crate::proxy::search::SongKey>,
    tracks: &mut Vec<WaveTrack>,
    seed: SeedTrack,
    count: usize,
) {
    let request_count = count.saturating_mul(2).clamp(count, MAX_NEXT_COUNT);
    let entries = match tokio::time::timeout(
        std::time::Duration::from_secs(SEED_EXTENSION_TIMEOUT_SECS),
        crate::proxy::similar::recommended_for_seed(
            state,
            username,
            &seed,
            request_count,
            existing,
        ),
    )
    .await
    {
        Ok(Ok(entries)) => entries,
        Ok(Err(error)) => {
            tracing::debug!(%error, artist = seed.artist, title = seed.title, "wave seed produced no recommendations");
            Vec::new()
        }
        Err(error) => {
            tracing::debug!(%error, artist = seed.artist, title = seed.title, "wave seed timed out");
            Vec::new()
        }
    };
    for entry in entries {
        if suppression.is_suppressed(&entry.artist, &entry.title) {
            continue;
        }
        existing.push(crate::proxy::search::SongKey::new(
            &entry.artist,
            &entry.title,
            entry.duration_secs,
        ));
        if !tracks.iter().any(|t| t.id == entry.id) {
            tracks.push(wave_track_from_entry(entry, auth_stream));
        }
        if tracks.len() >= count {
            break;
        }
    }
}

async fn seed_from_id(state: &AppState, id: &str) -> anyhow::Result<SeedTrack> {
    if vtrack::is_virtual_id(id) {
        let track = vtrack::get(&state.db, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("virtual seed not found"))?;
        return Ok(SeedTrack::from(track));
    }
    crate::proxy::similar::fetch_real_seed(state, id).await
}

async fn listen_seed_to_seed_track(state: &AppState, seed: crate::recs::ListenSeed) -> SeedTrack {
    if let Some(id) = &seed.subsonic_id {
        if let Ok(seed) = seed_from_id(state, id).await {
            return seed;
        }
    }
    SeedTrack {
        artist: seed.artist,
        title: seed.title,
        provider: "listen".into(),
        provider_track_id: seed
            .subsonic_id
            .unwrap_or_else(|| format!("listen:{}", uuid::Uuid::new_v4())),
    }
}

async fn positive_feedback_seeds(
    state: &AppState,
    username: &str,
    limit: usize,
) -> sqlx::Result<Vec<SeedTrack>> {
    let rows: Vec<(Option<String>, String, String)> = sqlx::query_as(
        "SELECT track_id, artist, title
         FROM wave_feedback
         WHERE username = ? AND action IN ('like', 'play')
         ORDER BY created_at_epoch DESC
         LIMIT ?",
    )
    .bind(username)
    .bind(limit as i64 * 3)
    .fetch_all(&state.db)
    .await?;
    let mut seen = std::collections::HashSet::new();
    let mut seeds = Vec::new();
    for (track_id, artist, title) in rows {
        let key = crate::recs::song_key(&artist, &title);
        if !seen.insert(key) {
            continue;
        }
        let seed = match track_id.as_deref() {
            Some(id) => seed_from_id(state, id).await.ok(),
            None => None,
        }
        .unwrap_or_else(|| SeedTrack {
            artist,
            title,
            provider: "feedback".into(),
            provider_track_id: track_id.unwrap_or_else(|| "feedback".into()),
        });
        seeds.push(seed);
        if seeds.len() >= limit {
            break;
        }
    }
    Ok(seeds)
}

async fn random_tracks(
    state: &AppState,
    auth_stream: &str,
    auth_json: &str,
    count: usize,
) -> anyhow::Result<Vec<WaveTrack>> {
    let url = format!(
        "{}/rest/getRandomSongs?{auth_json}&size={count}",
        state.config.navidrome.base_url.trim_end_matches('/')
    );
    let value: serde_json::Value = state
        .http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    anyhow::ensure!(
        value["subsonic-response"]["status"].as_str() == Some("ok"),
        "random songs failed"
    );
    let songs = &value["subsonic-response"]["randomSongs"]["song"];
    let items: Vec<serde_json::Value> = match songs {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(_) => vec![songs.clone()],
        _ => Vec::new(),
    };
    Ok(items
        .into_iter()
        .filter_map(|song| wave_track_from_json(&song, auth_stream))
        .collect())
}

async fn record_feedback(
    state: &AppState,
    username: &str,
    track_id: &str,
    action: &str,
) -> anyhow::Result<()> {
    let seed = seed_from_id(state, track_id).await?;
    let now = vtrack::epoch_secs();
    let song_key = crate::recs::song_key(&seed.artist, &seed.title);
    let artist_key = crate::recs::artist_key(&seed.artist);
    sqlx::query(
        "INSERT INTO wave_feedback
            (username, track_id, artist, title, song_key, artist_key, action, created_at_epoch)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(username, song_key, action) DO UPDATE SET
            track_id = excluded.track_id,
            artist = excluded.artist,
            title = excluded.title,
            artist_key = excluded.artist_key,
            created_at_epoch = excluded.created_at_epoch",
    )
    .bind(username)
    .bind(track_id)
    .bind(&seed.artist)
    .bind(&seed.title)
    .bind(&song_key)
    .bind(&artist_key)
    .bind(action)
    .bind(now)
    .execute(&state.db)
    .await?;
    if action == "play" {
        crate::recs::record_listen(
            &state.db,
            username,
            &seed.artist,
            &seed.title,
            Some(track_id),
            now,
        )
        .await?;
    }
    Ok(())
}

#[derive(Default)]
struct FeedbackSuppression {
    song_keys: std::collections::HashSet<String>,
    artist_keys: std::collections::HashSet<String>,
}

impl FeedbackSuppression {
    async fn load(state: &AppState, username: &str) -> Self {
        let now = vtrack::epoch_secs();
        let cutoff = now - FEEDBACK_DISLIKE_COOLDOWN_DAYS * 86_400;
        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT song_key, artist_key, action, created_at_epoch
             FROM wave_feedback
             WHERE username = ? AND action IN ('skip', 'dislike') AND created_at_epoch >= ?",
        )
        .bind(username)
        .bind(cutoff)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        let mut suppression = Self::default();
        for (song_key, artist_key, action, created_at) in rows {
            match action.as_str() {
                "dislike" => {
                    suppression.song_keys.insert(song_key);
                    suppression.artist_keys.insert(artist_key);
                }
                "skip" if created_at >= now - FEEDBACK_SKIP_COOLDOWN_DAYS * 86_400 => {
                    suppression.song_keys.insert(song_key);
                }
                _ => {}
            }
        }
        suppression
    }

    fn is_suppressed(&self, artist: &str, title: &str) -> bool {
        self.song_keys
            .contains(&crate::recs::song_key(artist, title))
            || self.artist_keys.contains(&crate::recs::artist_key(artist))
    }
}

fn wave_track_from_entry(entry: SongEntry, auth_stream: &str) -> WaveTrack {
    WaveTrack {
        stream_url: stream_url(auth_stream, &entry.id),
        id: entry.id,
        title: entry.title,
        artist: entry.artist,
        artist_id: None,
        album: entry.album,
        album_id: None,
        duration_secs: entry.duration_secs,
        cover_art: entry.cover_art,
    }
}

fn wave_track_from_json(song: &serde_json::Value, auth_stream: &str) -> Option<WaveTrack> {
    let id = song["id"].as_str()?.to_string();
    Some(WaveTrack {
        stream_url: stream_url(auth_stream, &id),
        id,
        title: song["title"].as_str().unwrap_or("Unknown").to_string(),
        artist: song["artist"]
            .as_str()
            .unwrap_or("Unknown artist")
            .to_string(),
        artist_id: song["artistId"].as_str().map(str::to_string),
        album: song["album"].as_str().map(str::to_string),
        album_id: song["albumId"].as_str().map(str::to_string),
        duration_secs: song["duration"].as_i64(),
        cover_art: song["coverArt"].as_str().map(str::to_string),
    })
}

fn stream_url(auth_stream: &str, id: &str) -> String {
    format!("/rest/stream?{auth_stream}&id={}", percent_encode(id))
}

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
}
