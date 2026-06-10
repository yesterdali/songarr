//! scrobble/star/unstar interception for virtual ids: accept, persist in
//! pending_actions (with the original auth params for post-import replay),
//! and report success. Real ids pass through; mixed batches are split.

use axum::extract::{Request, State};
use axum::response::Response;

use crate::subsonic::Format;
use crate::vtrack;
use crate::AppState;

use super::passthrough;

pub async fn scrobble_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, "scrobble").await
}

pub async fn star_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, "star").await
}

pub async fn unstar_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, "unstar").await
}

async fn handle(state: AppState, req: Request, action: &'static str) -> Response {
    let query = req.uri().query().unwrap_or("").to_owned();
    let (format, username, virtual_ids, real_ids) = {
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let format = Format::from_query_value(
            pairs.iter().find(|(k, _)| k == "f").map(|(_, v)| v.as_str()),
        );
        let username = pairs
            .iter()
            .find(|(k, _)| k == "u")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let (virtual_ids, real_ids): (Vec<String>, Vec<String>) = pairs
            .iter()
            .filter(|(k, _)| k == "id")
            .map(|(_, v)| v.clone())
            .partition(|id| vtrack::is_virtual_id(id));
        (format, username, virtual_ids, real_ids)
    };

    if virtual_ids.is_empty() {
        return passthrough::handler(State(state), req).await;
    }

    // Store the entire original query string: it carries u/t/s (Subsonic
    // tokens don't expire) plus scrobble extras like `time`/`submission`,
    // everything M4 needs to replay this as the requesting user.
    let payload = serde_json::json!({
        "endpoint": action,
        "query": query,
    })
    .to_string();
    for id in &virtual_ids {
        if let Err(error) = crate::pending::store(&state.db, id, &username, action, &payload).await
        {
            tracing::error!(%error, id, action, "failed to persist pending action");
        } else {
            tracing::info!(id, action, user = username, "stored pending action for virtual track");
        }
    }

    if !real_ids.is_empty() {
        // Mixed batch: forward the real ids, drop the virtual ones.
        let new_query = {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
                if key == "id" && vtrack::is_virtual_id(&value) {
                    continue;
                }
                serializer.append_pair(&key, &value);
            }
            serializer.finish()
        };
        let (mut parts, body) = req.into_parts();
        let uri = format!("{}?{}", parts.uri.path(), new_query);
        if let Ok(uri) = uri.parse() {
            parts.uri = uri;
        }
        return passthrough::handler(State(state), Request::from_parts(parts, body)).await;
    }

    // All ids virtual: plain empty success.
    state
        .envelope()
        .await
        .render_ok(format.unwrap_or(Format::Xml), None)
}
