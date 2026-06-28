use super::*;

/// Record the caller's current track (Friend Activity). Auth in the query
/// string, the track in the JSON body.
pub async fn now_playing_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<NowPlayingRequest>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    if body.id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "missing id").into_response();
    }
    let result = sqlx::query(
        "INSERT INTO user_activity
            (username, song_id, title, artist, album, cover_art, updated_at_epoch)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(username) DO UPDATE SET
            song_id = excluded.song_id,
            title = excluded.title,
            artist = excluded.artist,
            album = excluded.album,
            cover_art = excluded.cover_art,
            updated_at_epoch = excluded.updated_at_epoch",
    )
    .bind(&username)
    .bind(body.id.trim())
    .bind(&body.title)
    .bind(&body.artist)
    .bind(&body.album)
    .bind(&body.cover_art)
    .bind(vtrack::epoch_secs())
    .execute(&state.db)
    .await;
    match result {
        Ok(_) => Json(serde_json::json!({ "status": "ok" })).into_response(),
        Err(error) => {
            tracing::warn!(%error, username, "now-playing update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

/// What everyone else on the instance is listening to (Friend Activity feed).
pub async fn friends_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let rows = sqlx::query_as::<_, FriendRow>(
        "SELECT username, song_id, title, artist, album, cover_art, updated_at_epoch
         FROM user_activity
         WHERE username <> ?
         ORDER BY updated_at_epoch DESC
         LIMIT 50",
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let names = display_names(&state.db).await;
    let friends: Vec<FriendActivity> = rows
        .into_iter()
        .map(|row| FriendActivity {
            display_name: names.get(&row.username).cloned(),
            username: row.username,
            id: row.song_id,
            title: row.title,
            artist: row.artist,
            album: row.album,
            cover_art: row.cover_art,
            updated_at: row.updated_at_epoch,
        })
        .collect();
    Json(FriendsResponse { friends }).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingRequest {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    cover_art: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct FriendRow {
    username: String,
    song_id: String,
    title: String,
    artist: String,
    album: Option<String>,
    cover_art: Option<String>,
    updated_at_epoch: i64,
}

#[derive(Debug, Serialize)]
struct FriendsResponse {
    friends: Vec<FriendActivity>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendActivity {
    username: String,
    display_name: Option<String>,
    id: String,
    title: String,
    artist: String,
    album: Option<String>,
    cover_art: Option<String>,
    updated_at: i64,
}

// ---- Personalization: display name + avatar ----

const MAX_AVATAR_BYTES: usize = 256 * 1024;
const MAX_DISPLAY_NAME: usize = 40;

pub async fn profile_get_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let row: Option<(Option<String>, i64)> = sqlx::query_as(
        "SELECT display_name, (avatar_blob IS NOT NULL) FROM user_profile WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let (display_name, has_avatar) = match row {
        Some((dn, has)) => (dn, has != 0),
        None => (None, false),
    };
    Json(serde_json::json!({ "displayName": display_name, "hasAvatar": has_avatar }))
        .into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpdate {
    #[serde(default)]
    display_name: Option<String>,
}

pub async fn profile_set_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    Json(body): Json<ProfileUpdate>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let name = body
        .display_name
        .map(|s| s.trim().chars().take(MAX_DISPLAY_NAME).collect::<String>())
        .filter(|s| !s.is_empty());
    let result = sqlx::query(
        "INSERT INTO user_profile (username, display_name, updated_at_epoch)
         VALUES (?, ?, ?)
         ON CONFLICT(username) DO UPDATE SET
            display_name = excluded.display_name,
            updated_at_epoch = excluded.updated_at_epoch",
    )
    .bind(&username)
    .bind(&name)
    .bind(vtrack::epoch_secs())
    .execute(&state.db)
    .await;
    match result {
        Ok(_) => Json(serde_json::json!({ "status": "ok", "displayName": name })).into_response(),
        Err(error) => {
            tracing::warn!(%error, username, "profile update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

pub async fn avatar_set_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty image").into_response();
    }
    if body.len() > MAX_AVATAR_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            "avatar too large (max 256 KB)",
        )
            .into_response();
    }
    let mime = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|m| m.starts_with("image/"))
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = body.to_vec();
    let result = sqlx::query(
        "INSERT INTO user_profile (username, avatar_blob, avatar_mime, updated_at_epoch)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(username) DO UPDATE SET
            avatar_blob = excluded.avatar_blob,
            avatar_mime = excluded.avatar_mime,
            updated_at_epoch = excluded.updated_at_epoch",
    )
    .bind(&username)
    .bind(&bytes)
    .bind(&mime)
    .bind(vtrack::epoch_secs())
    .execute(&state.db)
    .await;
    match result {
        Ok(_) => Json(serde_json::json!({ "status": "ok" })).into_response(),
        Err(error) => {
            tracing::warn!(%error, username, "avatar update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

pub async fn avatar_delete_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let _ = sqlx::query(
        "UPDATE user_profile SET avatar_blob = NULL, avatar_mime = NULL, updated_at_epoch = ?
         WHERE username = ?",
    )
    .bind(vtrack::epoch_secs())
    .bind(&username)
    .execute(&state.db)
    .await;
    Json(serde_json::json!({ "status": "ok" })).into_response()
}

/// Serve a user's avatar (any authed caller may read any user's, like cover art).
pub async fn avatar_get_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if authenticated(&state, &params).await.is_err() {
        return (StatusCode::UNAUTHORIZED, "auth failed").into_response();
    }
    let user = params.get("user").cloned().unwrap_or_default();
    if user.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let row: Option<(Vec<u8>, Option<String>)> = sqlx::query_as(
        "SELECT avatar_blob, avatar_mime FROM user_profile
         WHERE username = ? AND avatar_blob IS NOT NULL",
    )
    .bind(&user)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    match row {
        Some((blob, mime)) => {
            let mut response = Response::new(Body::from(blob));
            let mime = mime.unwrap_or_else(|| "image/jpeg".into());
            response.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("image/jpeg")),
            );
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=60"),
            );
            response
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// username → display_name for users who set one (for friend/member lists).
pub(super) async fn display_names(db: &sqlx::SqlitePool) -> HashMap<String, String> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT username, display_name FROM user_profile WHERE display_name IS NOT NULL",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|(user, name)| name.map(|n| (user, n)))
    .collect()
}
