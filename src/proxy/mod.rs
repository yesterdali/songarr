pub mod album;
pub mod artist;
pub mod coverart;
pub mod lyrics;
pub mod passthrough;
pub mod playlists;
pub mod scrobble;
pub mod search;
pub mod similar;
pub mod song;
pub mod stream;
pub mod wave;

use axum::extract::{Request, State};
use axum::response::Response;

use crate::subsonic::types::SongEntry;
use crate::vtrack::VirtualTrack;
use crate::AppState;

/// Replace the `id` query param and forward to Navidrome — used once a
/// virtual track has been imported and maps to a real subsonic id. Clients
/// don't reliably follow redirects, so the rewrite happens server-side.
pub(crate) async fn rewrite_id_and_passthrough(
    state: AppState,
    req: Request,
    new_id: &str,
) -> Response {
    let (mut parts, body) = req.into_parts();

    // Scoped: the Serializer is !Send and must not live across the await.
    let new_query = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        let mut replaced = false;
        for (key, value) in url::form_urlencoded::parse(parts.uri.query().unwrap_or("").as_bytes())
        {
            if key == "id" {
                serializer.append_pair("id", new_id);
                replaced = true;
            } else {
                serializer.append_pair(&key, &value);
            }
        }
        if !replaced {
            serializer.append_pair("id", new_id);
        }
        serializer.finish()
    };
    let new_uri = format!("{}?{}", parts.uri.path(), new_query);
    match new_uri.parse() {
        Ok(uri) => parts.uri = uri,
        Err(error) => {
            tracing::error!(%error, %new_uri, "id rewrite produced invalid uri");
        }
    }

    passthrough::handler(State(state), Request::from_parts(parts, body)).await
}

pub(crate) async fn song_entry_with_repaired_artwork(
    state: &AppState,
    mut track: VirtualTrack,
) -> SongEntry {
    if track.artwork_url.is_none() {
        if let Err(error) = crate::valbum::repair_track_artwork(&state.db, &mut track).await {
            tracing::debug!(
                %error,
                id = track.id,
                "virtual track artwork repair failed"
            );
        }
    }
    SongEntry::from_virtual(&track, &state.config.streaming)
}

#[cfg(test)]
mod tests {
    #[test]
    fn id_rewrite_preserves_other_params() {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in
            url::form_urlencoded::parse("u=alice&t=ab12&s=xy&id=sgr_old&f=json".as_bytes())
        {
            if key == "id" {
                serializer.append_pair("id", "real99");
            } else {
                serializer.append_pair(&key, &value);
            }
        }
        let query = serializer.finish();
        assert!(query.contains("id=real99"));
        assert!(query.contains("u=alice"));
        assert!(query.contains("t=ab12"));
        assert!(!query.contains("sgr_old"));
    }
}
