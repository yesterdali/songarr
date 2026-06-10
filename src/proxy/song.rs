//! getSong interception: synthesize a response for virtual ids, pass
//! everything else through. After import, the id is rewritten to the real
//! track and proxied (M4).

use axum::extract::{Request, State};
use axum::response::Response;

use crate::subsonic::types::SongEntry;
use crate::subsonic::{auth, error_not_found, Format};
use crate::vtrack;
use crate::AppState;

use super::passthrough;

pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let (id, format) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            params.get("id").map(|v| v.to_string()).unwrap_or_default(),
            Format::from_query_value(params.get("f").map(|v| v.as_ref())),
        )
    };

    let (Some(format), true) = (format, vtrack::is_virtual_id(&id)) else {
        return passthrough::handler(State(state), req).await;
    };

    let envelope = state.envelope().await;
    match vtrack::get(&state.db, &id).await {
        Ok(Some(track)) if track.real_subsonic_id.is_none() => {
            let entry = SongEntry::from_virtual(&track, &state.config.streaming);
            envelope.render_ok(format, Some(entry.into_payload()))
        }
        Ok(Some(track)) => {
            // Imported: serve the real track's metadata from Navidrome by
            // rewriting the id (clients don't follow redirects reliably).
            let real_id = track.real_subsonic_id.unwrap();
            super::rewrite_id_and_passthrough(state, req, &real_id).await
        }
        Ok(None) => error_not_found(&envelope, format),
        Err(error) => {
            tracing::error!(%error, id, "getSong db lookup failed");
            envelope.render_error(format, 0, "internal error")
        }
    }
}
