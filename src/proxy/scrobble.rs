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
            pairs
                .iter()
                .find(|(k, _)| k == "f")
                .map(|(_, v)| v.as_str()),
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
        if action == "scrobble" {
            log_real_listens(&state, &username, &real_ids, &query).await;
        }
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
        if action == "scrobble" {
            log_virtual_listen(&state, &username, id, &query).await;
        }
        if let Err(error) = crate::pending::store(&state.db, id, &username, action, &payload).await
        {
            tracing::error!(%error, id, action, "failed to persist pending action");
        } else {
            tracing::info!(
                id,
                action,
                user = username,
                "stored pending action for virtual track"
            );
        }
    }

    if !real_ids.is_empty() {
        if action == "scrobble" {
            log_real_listens(&state, &username, &real_ids, &query).await;
        }
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

async fn log_virtual_listen(state: &AppState, username: &str, id: &str, query: &str) {
    match vtrack::get(&state.db, id).await {
        Ok(Some(track)) => {
            let listened_at = scrobble_time_epoch(query);
            if let Err(error) = crate::recs::record_listen(
                &state.db,
                username,
                &track.artist,
                &track.title,
                Some(id),
                listened_at,
            )
            .await
            {
                tracing::warn!(%error, id, "failed to record virtual listen");
            }
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, id, "failed to load virtual track for listen log"),
    }
}

async fn log_real_listens(state: &AppState, username: &str, ids: &[String], query: &str) {
    let listened_at = scrobble_time_epoch(query);
    for id in ids {
        match fetch_real_song_metadata(state, id).await {
            Ok(Some((artist, title))) => {
                if let Err(error) = crate::recs::record_listen(
                    &state.db,
                    username,
                    &artist,
                    &title,
                    Some(id),
                    listened_at,
                )
                .await
                {
                    tracing::warn!(%error, id, "failed to record real listen");
                }
            }
            Ok(None) => {}
            Err(error) => tracing::debug!(%error, id, "real listen metadata lookup failed"),
        }
    }
}

async fn fetch_real_song_metadata(
    state: &AppState,
    id: &str,
) -> anyhow::Result<Option<(String, String)>> {
    let encoded_id: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
    let url = format!(
        "{}/rest/getSong?{}&id={encoded_id}&f=json",
        state.config.navidrome.base_url.trim_end_matches('/'),
        crate::subsonic::auth::admin_auth_query(&state.config.navidrome)
    );
    let value: serde_json::Value = state
        .http
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let song = &value["subsonic-response"]["song"];
    let Some(artist) = song["artist"].as_str() else {
        return Ok(None);
    };
    let Some(title) = song["title"].as_str() else {
        return Ok(None);
    };
    Ok(Some((artist.to_string(), title.to_string())))
}

fn scrobble_time_epoch(query: &str) -> i64 {
    let raw = url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "time")
        .and_then(|(_, value)| value.parse::<i64>().ok());
    match raw {
        Some(value) if value > 10_000_000_000 => value / 1000,
        Some(value) if value > 0 => value,
        _ => crate::vtrack::epoch_secs(),
    }
}

#[cfg(test)]
mod tests {
    use super::scrobble_time_epoch;

    #[test]
    fn scrobble_time_accepts_seconds_or_millis() {
        assert_eq!(scrobble_time_epoch("time=1700000000"), 1_700_000_000);
        assert_eq!(scrobble_time_epoch("time=1700000000123"), 1_700_000_000);
    }
}
