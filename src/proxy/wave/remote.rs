use super::*;

// ---- Remote control (Spotify-Connect-style; see discord-remote-plan.md) ----

/// A remote_state row older than this is treated as disconnected.
const REMOTE_ALIVE_SECS: i64 = 15;
const REMOTE_ACTIONS: &[&str] = &[
    "connect",
    "disconnect",
    "play",
    "pause",
    "resume",
    "next",
    "prev",
    "seek",
];

/// App → bot: enqueue a remote command.
pub async fn remote_command_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<RemoteCommandRequest>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    if !REMOTE_ACTIONS.contains(&body.action.as_str()) {
        return (StatusCode::BAD_REQUEST, "unknown action").into_response();
    }
    let payload = body.payload.map(|value| value.to_string());
    let result = sqlx::query(
        "INSERT INTO remote_command (username, action, payload, created_at_epoch)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&username)
    .bind(&body.action)
    .bind(&payload)
    .bind(vtrack::epoch_secs())
    .execute(&state.db)
    .await;
    match result {
        Ok(_) => {
            // Wake the bot's long-poll immediately (push-like control).
            state
                .remote_waiters(&username)
                .await
                .commands
                .notify_waiters();
            Json(serde_json::json!({ "status": "ok" })).into_response()
        }
        Err(error) => {
            tracing::warn!(%error, username, action = body.action, "remote command insert failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

async fn fetch_remote_commands(
    db: &sqlx::SqlitePool,
    username: &str,
    after: i64,
) -> Vec<RemoteCommandRow> {
    sqlx::query_as::<_, RemoteCommandRow>(
        "SELECT seq, action, payload FROM remote_command
         WHERE username = ? AND seq > ? ORDER BY seq LIMIT 100",
    )
    .bind(username)
    .bind(after)
    .fetch_all(db)
    .await
    .unwrap_or_default()
}

/// Bot → poll: fetch commands newer than `after`. With `wait=<secs>` this
/// long-polls — it blocks until a command arrives (or the timeout), so control
/// is effectively pushed rather than polled.
pub async fn remote_commands_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let after: i64 = params
        .get("after")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let wait: u64 = params
        .get("wait")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .min(30);
    // Everything <= after is acknowledged; prune it so the log stays small.
    let _ = sqlx::query("DELETE FROM remote_command WHERE username = ? AND seq <= ?")
        .bind(&username)
        .bind(after)
        .execute(&state.db)
        .await;

    let waiters = state.remote_waiters(&username).await;
    // Register the wakeup BEFORE the first read so a command posted in between
    // still resolves the wait (no lost notification).
    let notified = waiters.commands.notified();
    tokio::pin!(notified);
    let mut rows = fetch_remote_commands(&state.db, &username, after).await;
    if rows.is_empty() && wait > 0 {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(wait), notified).await;
        rows = fetch_remote_commands(&state.db, &username, after).await;
    }

    let commands: Vec<_> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "seq": row.seq,
                "action": row.action,
                "payload": row
                    .payload
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(&p).ok()),
            })
        })
        .collect();
    Json(serde_json::json!({ "commands": commands })).into_response()
}

/// Bot → state: report current playback (also a heartbeat).
pub async fn remote_state_report_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<RemoteStateReport>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let now = vtrack::epoch_secs();
    let queue_json = body.queue.as_ref().map(|q| q.to_string());
    let result = sqlx::query(
        "INSERT INTO remote_state
            (username, connected, track_id, title, artist, album, cover_art,
             position_ms, duration_ms, is_playing, queue_json, updated_at_epoch, busy, rev)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
         ON CONFLICT(username) DO UPDATE SET
            connected = excluded.connected,
            track_id = excluded.track_id,
            title = excluded.title,
            artist = excluded.artist,
            album = excluded.album,
            cover_art = excluded.cover_art,
            position_ms = excluded.position_ms,
            duration_ms = excluded.duration_ms,
            is_playing = excluded.is_playing,
            queue_json = excluded.queue_json,
            updated_at_epoch = excluded.updated_at_epoch,
            busy = excluded.busy,
            rev = rev + 1",
    )
    .bind(&username)
    .bind(body.connected as i64)
    .bind(&body.track_id)
    .bind(&body.title)
    .bind(&body.artist)
    .bind(&body.album)
    .bind(&body.cover_art)
    .bind(body.position_ms)
    .bind(body.duration_ms)
    .bind(body.is_playing as i64)
    .bind(&queue_json)
    .bind(now)
    .bind(body.busy as i64)
    .execute(&state.db)
    .await;
    // Keep Friend Activity in sync: a remote play is still this user's "now
    // playing" (the local client isn't reporting while remote).
    if body.connected && body.is_playing {
        if let Some(id) = body.track_id.as_deref() {
            let _ = sqlx::query(
                "INSERT INTO user_activity
                    (username, song_id, title, artist, album, cover_art, updated_at_epoch)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(username) DO UPDATE SET
                    song_id = excluded.song_id, title = excluded.title,
                    artist = excluded.artist, album = excluded.album,
                    cover_art = excluded.cover_art, updated_at_epoch = excluded.updated_at_epoch",
            )
            .bind(&username)
            .bind(id)
            .bind(body.title.as_deref().unwrap_or("Unknown"))
            .bind(body.artist.as_deref().unwrap_or("Unknown artist"))
            .bind(&body.album)
            .bind(&body.cover_art)
            .bind(now)
            .execute(&state.db)
            .await;
        }
    }
    match result {
        Ok(_) => {
            // Wake the app's state long-poll immediately.
            state.remote_waiters(&username).await.state.notify_waiters();
            Json(serde_json::json!({ "status": "ok" })).into_response()
        }
        Err(error) => {
            tracing::warn!(%error, username, "remote state report failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

async fn fetch_remote_state(db: &sqlx::SqlitePool, username: &str) -> Option<RemoteStateRow> {
    sqlx::query_as::<_, RemoteStateRow>(
        "SELECT connected, track_id, title, artist, album, cover_art,
                position_ms, duration_ms, is_playing, queue_json, updated_at_epoch, busy, rev
         FROM remote_state WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

/// App → state: read the bot's reported playback for the playbar. With
/// `wait=<secs>&since=<rev>` this long-polls until the state advances past `rev`.
pub async fn remote_state_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let since: i64 = params
        .get("since")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let wait: u64 = params
        .get("wait")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .min(30);

    let waiters = state.remote_waiters(&username).await;
    let notified = waiters.state.notified();
    tokio::pin!(notified);
    let mut row = fetch_remote_state(&state.db, &username).await;
    let fresh = |r: &Option<RemoteStateRow>| r.as_ref().map(|x| x.rev).unwrap_or(0) > since;
    if !fresh(&row) && wait > 0 {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(wait), notified).await;
        row = fetch_remote_state(&state.db, &username).await;
    }
    let now = vtrack::epoch_secs();
    let response = match row {
        Some(r) => {
            let alive = now - r.updated_at_epoch <= REMOTE_ALIVE_SECS;
            RemoteStateResponse {
                connected: alive && r.connected != 0,
                track_id: r.track_id,
                title: r.title,
                artist: r.artist,
                album: r.album,
                cover_art: r.cover_art,
                position_ms: r.position_ms,
                duration_ms: r.duration_ms,
                is_playing: alive && r.is_playing != 0,
                queue: r
                    .queue_json
                    .and_then(|q| serde_json::from_str::<serde_json::Value>(&q).ok()),
                updated_at: r.updated_at_epoch,
                rev: r.rev,
                busy: alive && r.busy != 0,
            }
        }
        None => RemoteStateResponse {
            updated_at: now,
            ..Default::default()
        },
    };
    Json(response).into_response()
}

#[derive(Debug, Deserialize)]
pub struct RemoteCommandRequest {
    action: String,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct RemoteCommandRow {
    seq: i64,
    action: String,
    payload: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStateReport {
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    track_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    cover_art: Option<String>,
    #[serde(default)]
    position_ms: Option<i64>,
    #[serde(default)]
    duration_ms: Option<i64>,
    #[serde(default)]
    is_playing: bool,
    #[serde(default)]
    queue: Option<serde_json::Value>,
    #[serde(default)]
    busy: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct RemoteStateRow {
    connected: i64,
    track_id: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    cover_art: Option<String>,
    position_ms: Option<i64>,
    duration_ms: Option<i64>,
    is_playing: i64,
    queue_json: Option<String>,
    updated_at_epoch: i64,
    busy: i64,
    rev: i64,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteStateResponse {
    connected: bool,
    track_id: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    cover_art: Option<String>,
    position_ms: Option<i64>,
    duration_ms: Option<i64>,
    is_playing: bool,
    queue: Option<serde_json::Value>,
    updated_at: i64,
    rev: i64,
    busy: bool,
}
