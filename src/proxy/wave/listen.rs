use super::*;

// ---- Listen Together (synced group listening; see listen-together-plan.md) ----
//
// A session is a *virtual transport*: it stores "track[index] is at position P
// as of server-time T, playing?". No server audio — each client slaves its own
// <audio> to this timeline. play/pause/seek/next just re-anchor (P, T).

const LISTEN_MEMBER_TTL_MS: i64 = 30_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenTrack {
    pub(super) id: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) artist: String,
    #[serde(default)]
    pub(super) provider: Option<String>,
    #[serde(default)]
    pub(super) artist_id: Option<String>,
    #[serde(default)]
    pub(super) album: Option<String>,
    #[serde(default)]
    pub(super) album_id: Option<String>,
    #[serde(default)]
    pub(super) cover_art: Option<String>,
    #[serde(default)]
    #[serde(alias = "duration")]
    pub(super) duration_ms: Option<i64>,
}

pub struct ListenSession {
    pub(super) code: String,
    pub(super) host: String,
    pub(super) members: HashMap<String, i64>, // username → last_seen_ms
    pub(super) tracks: Vec<ListenTrack>,
    pub(super) events: Vec<ListenEvent>,
    pub(super) index: usize,
    pub(super) anchor_pos_ms: i64,
    pub(super) anchor_ts_ms: i64,
    pub(super) is_playing: bool,
    pub(super) rev: i64,
}

impl ListenSession {
    fn live_pos_ms(&self) -> i64 {
        if self.is_playing {
            self.anchor_pos_ms + (now_ms() - self.anchor_ts_ms)
        } else {
            self.anchor_pos_ms
        }
    }
    fn current(&self) -> Option<&ListenTrack> {
        self.tracks.get(self.index)
    }
}

pub struct ListenRoom {
    state: Mutex<ListenSession>,
    notify: Notify,
}

pub type ListenSessions = Arc<Mutex<HashMap<String, Arc<ListenRoom>>>>;

pub(super) struct ListenSnapshot {
    code: String,
    host: String,
    members: Vec<String>,
    track: Option<ListenTrack>,
    queue: Vec<ListenTrack>,
    pub(super) events: Vec<ListenEvent>,
    anchor_pos_ms: i64,
    anchor_ts_ms: i64,
    is_playing: bool,
    rev: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenEvent {
    pub(super) id: i64,
    pub(super) username: String,
    pub(super) kind: String,
    pub(super) text: String,
    pub(super) at_ms: i64,
}

pub(super) fn snapshot(s: &ListenSession) -> ListenSnapshot {
    ListenSnapshot {
        code: s.code.clone(),
        host: s.host.clone(),
        members: s.members.keys().cloned().collect(),
        track: s.current().cloned(),
        queue: s.tracks.iter().skip(s.index + 1).cloned().collect(),
        events: {
            let mut events: Vec<_> = s.events.iter().rev().take(50).cloned().collect();
            events.reverse();
            events
        },
        anchor_pos_ms: s.anchor_pos_ms,
        anchor_ts_ms: s.anchor_ts_ms,
        is_playing: s.is_playing,
        rev: s.rev,
    }
}

async fn snapshot_json(state: &AppState, snap: ListenSnapshot, me: &str) -> serde_json::Value {
    let names = display_names(&state.db).await;
    let members: Vec<_> = snap
        .members
        .iter()
        .map(|u| serde_json::json!({ "username": u, "displayName": names.get(u) }))
        .collect();
    serde_json::json!({
        "code": snap.code,
        "host": snap.host,
        "isHost": snap.host == me,
        "members": members,
        "track": snap.track,
        "queue": snap.queue,
        "events": snap.events,
        "anchorPosMs": snap.anchor_pos_ms,
        "anchorTsMs": snap.anchor_ts_ms,
        "serverMs": now_ms(),
        "isPlaying": snap.is_playing,
        "rev": snap.rev,
    })
}

async fn listen_room(state: &AppState, code: &str) -> Option<Arc<ListenRoom>> {
    state.listen_sessions.lock().await.get(code).cloned()
}

fn new_listen_code() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..6].to_uppercase()
}

/// Server time, for the client's clock-offset estimation.
pub async fn listen_time_handler() -> Response {
    Json(serde_json::json!({ "serverMs": now_ms() })).into_response()
}

pub async fn listen_create_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let now = now_ms();
    let mut map = state.listen_sessions.lock().await;
    let code = loop {
        let c = new_listen_code();
        if !map.contains_key(&c) {
            break c;
        }
    };
    let session = ListenSession {
        code: code.clone(),
        host: username.clone(),
        members: HashMap::from([(username, now)]),
        tracks: Vec::new(),
        events: Vec::new(),
        index: 0,
        anchor_pos_ms: 0,
        anchor_ts_ms: now,
        is_playing: false,
        rev: 1,
    };
    map.insert(
        code.clone(),
        Arc::new(ListenRoom {
            state: Mutex::new(session),
            notify: Notify::new(),
        }),
    );
    Json(serde_json::json!({ "code": code })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ListenJoinRequest {
    code: String,
}

pub async fn listen_join_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<ListenJoinRequest>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let code = body.code.trim().to_uppercase();
    let Some(room) = listen_room(&state, &code).await else {
        return (StatusCode::NOT_FOUND, "no such room").into_response();
    };
    let snap = {
        let mut s = room.state.lock().await;
        s.members.insert(username.clone(), now_ms());
        s.rev += 1;
        snapshot(&s)
    };
    room.notify.notify_waiters();
    Json(snapshot_json(&state, snap, &username).await).into_response()
}

pub async fn listen_leave_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<ListenJoinRequest>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let code = body.code.trim().to_uppercase();
    if let Some(room) = listen_room(&state, &code).await {
        let empty = {
            let mut s = room.state.lock().await;
            s.members.remove(&username);
            s.rev += 1;
            s.members.is_empty()
        };
        room.notify.notify_waiters();
        if empty {
            state.listen_sessions.lock().await.remove(&code);
        }
    }
    Json(serde_json::json!({ "status": "ok" })).into_response()
}

/// Long-poll the session (waits until rev > since). Also refreshes presence.
pub async fn listen_state_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let code = params
        .get("code")
        .cloned()
        .unwrap_or_default()
        .to_uppercase();
    let Some(room) = listen_room(&state, &code).await else {
        return Json(serde_json::json!({ "gone": true })).into_response();
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

    let notified = room.notify.notified();
    tokio::pin!(notified);
    let fresh = {
        let mut s = room.state.lock().await;
        s.members.insert(username.clone(), now_ms()); // presence (no rev bump)
        s.rev > since
    };
    if !fresh && wait > 0 {
        let _ = tokio::time::timeout(Duration::from_secs(wait), notified).await;
    }
    let snap = {
        let s = room.state.lock().await;
        snapshot(&s)
    };
    Json(snapshot_json(&state, snap, &username).await).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ListenCommandRequest {
    action: String,
    #[serde(default)]
    payload: serde_json::Value,
}

fn listen_track_from_wave(track: WaveTrack) -> ListenTrack {
    ListenTrack {
        id: track.id,
        title: track.title,
        artist: track.artist,
        provider: track.provider,
        artist_id: track.artist_id,
        album: track.album,
        album_id: track.album_id,
        cover_art: track.cover_art,
        duration_ms: track.duration_secs.map(|s| s * 1000),
    }
}

pub async fn listen_command_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<ListenCommandRequest>,
) -> Response {
    let (username, auth_stream, auth_json) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let code = params
        .get("code")
        .cloned()
        .unwrap_or_default()
        .to_uppercase();
    let Some(room) = listen_room(&state, &code).await else {
        return (StatusCode::NOT_FOUND, "no such room").into_response();
    };
    // Wave needs a (network) fetch before we take the session lock.
    let wave_tracks: Option<Vec<ListenTrack>> = if body.action == "wave" {
        next_tracks(&state, &username, &auth_stream, &auth_json, None, 12)
            .await
            .ok()
            .map(|tracks| tracks.into_iter().map(listen_track_from_wave).collect())
    } else {
        None
    };
    {
        let mut s = room.state.lock().await;
        let now = now_ms();
        match body.action.as_str() {
            "play" => {
                let tracks: Vec<ListenTrack> = body
                    .payload
                    .get("tracks")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                let start = body
                    .payload
                    .get("startIndex")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                if !tracks.is_empty() {
                    s.index = start.min(tracks.len() - 1);
                    s.tracks = tracks;
                    s.anchor_pos_ms = 0;
                    s.anchor_ts_ms = now;
                    s.is_playing = true;
                }
            }
            "wave" => {
                if let Some(tracks) = wave_tracks.filter(|t| !t.is_empty()) {
                    s.tracks = tracks;
                    s.index = 0;
                    s.anchor_pos_ms = 0;
                    s.anchor_ts_ms = now;
                    s.is_playing = true;
                }
            }
            "pause" => {
                s.anchor_pos_ms = s.live_pos_ms();
                s.anchor_ts_ms = now;
                s.is_playing = false;
            }
            "resume" => {
                s.anchor_ts_ms = now;
                s.is_playing = true;
            }
            "seek" => {
                if let Some(p) = body.payload.get("positionMs").and_then(|v| v.as_i64()) {
                    s.anchor_pos_ms = p.max(0);
                    s.anchor_ts_ms = now;
                }
            }
            "next" => {
                if s.index + 1 < s.tracks.len() {
                    s.index += 1;
                    s.anchor_pos_ms = 0;
                    s.anchor_ts_ms = now;
                    s.is_playing = true;
                } else {
                    s.is_playing = false;
                }
            }
            "prev" => {
                if s.live_pos_ms() > 3000 {
                    s.anchor_pos_ms = 0;
                    s.anchor_ts_ms = now;
                } else if s.index > 0 {
                    s.index -= 1;
                    s.anchor_pos_ms = 0;
                    s.anchor_ts_ms = now;
                    s.is_playing = true;
                } else {
                    s.anchor_pos_ms = 0;
                    s.anchor_ts_ms = now;
                }
            }
            "reaction" => {
                if let Some(text) = body.payload.get("emoji").and_then(|v| v.as_str()) {
                    push_listen_event(&mut s, &username, "reaction", text, now);
                }
            }
            "chat" => {
                if let Some(text) = body.payload.get("text").and_then(|v| v.as_str()) {
                    let text = text.trim();
                    if !text.is_empty() {
                        push_listen_event(&mut s, &username, "chat", text, now);
                    }
                }
            }
            other => tracing::debug!(action = other, "listen: unknown command"),
        }
        s.rev += 1;
    }
    room.notify.notify_waiters();
    Json(serde_json::json!({ "status": "ok" })).into_response()
}

pub(super) fn push_listen_event(
    session: &mut ListenSession,
    username: &str,
    kind: &str,
    text: &str,
    now: i64,
) {
    let text = text.chars().take(280).collect::<String>();
    let id = session.events.last().map(|event| event.id + 1).unwrap_or(1);
    session.events.push(ListenEvent {
        id,
        username: username.to_string(),
        kind: kind.to_string(),
        text,
        at_ms: now,
    });
    if session.events.len() > 100 {
        let drop_count = session.events.len() - 100;
        session.events.drain(0..drop_count);
    }
}

/// Advance finished tracks on the virtual timeline + prune empty/stale sessions.
pub async fn listen_sweeper(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let rooms: Vec<(String, Arc<ListenRoom>)> = {
            state
                .listen_sessions
                .lock()
                .await
                .iter()
                .map(|(c, r)| (c.clone(), r.clone()))
                .collect()
        };
        let mut to_remove = Vec::new();
        for (code, room) in rooms {
            let (changed, empty) = {
                let mut s = room.state.lock().await;
                let now = now_ms();
                s.members
                    .retain(|_, last| now - *last <= LISTEN_MEMBER_TTL_MS);
                let empty = s.members.is_empty();
                let mut changed = false;
                if !empty && s.is_playing {
                    if let Some(dur) = s.current().and_then(|t| t.duration_ms) {
                        if dur > 0 && s.live_pos_ms() >= dur {
                            if s.index + 1 < s.tracks.len() {
                                s.index += 1;
                                s.anchor_pos_ms = 0;
                                s.anchor_ts_ms += dur; // next starts where the last ended
                            } else {
                                s.is_playing = false;
                                s.anchor_pos_ms = dur;
                            }
                            s.rev += 1;
                            changed = true;
                        }
                    }
                }
                (changed, empty)
            };
            if changed {
                room.notify.notify_waiters();
            }
            if empty {
                to_remove.push(code);
            }
        }
        if !to_remove.is_empty() {
            let mut map = state.listen_sessions.lock().await;
            for code in to_remove {
                map.remove(&code);
            }
        }
    }
}
