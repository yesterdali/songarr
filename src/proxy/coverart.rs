//! getCoverArt for virtual ids: fetch provider artwork once, cache on disk,
//! serve from cache thereafter. Real ids pass through.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::subsonic::{auth, error_not_found, Format};
use crate::vtrack::{self, VirtualTrack};
use crate::AppState;

use super::passthrough;

pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let (id, format) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            params.get("id").map(|v| v.to_string()).unwrap_or_default(),
            Format::from_query_value(params.get("f").map(|v| v.as_ref())).unwrap_or(Format::Xml),
        )
    };

    if !vtrack::is_virtual_id(&id) {
        return passthrough::handler(State(state), req).await;
    }

    let track = match vtrack::get(&state.db, &id).await {
        Ok(Some(track)) => track,
        Ok(None) => return error_not_found(&state.envelope().await, format),
        Err(error) => {
            tracing::error!(%error, id, "getCoverArt db lookup failed");
            return error_not_found(&state.envelope().await, format);
        }
    };

    // Imported tracks defer to Navidrome's art for the real id. The real id
    // is a song id, which getCoverArt accepts (`mf-` lookup happens upstream).
    if let Some(real_id) = track.real_subsonic_id.clone() {
        return super::rewrite_id_and_passthrough(state, req, &real_id).await;
    }

    match serve_cached_artwork(&state, &track).await {
        Ok(Some(response)) => response,
        Ok(None) => error_not_found(&state.envelope().await, format),
        Err(error) => {
            tracing::warn!(%error, id, "virtual cover art fetch failed");
            error_not_found(&state.envelope().await, format)
        }
    }
}

async fn serve_cached_artwork(
    state: &AppState,
    track: &VirtualTrack,
) -> anyhow::Result<Option<Response>> {
    let Some(artwork_url) = &track.artwork_url else {
        return Ok(None);
    };

    let cache_path = state.artwork_cache_dir().join(format!("{}.img", track.id));
    let bytes = if let Ok(cached) = tokio::fs::read(&cache_path).await {
        cached
    } else {
        let response = state
            .http
            .get(artwork_url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?;
        let bytes = response.bytes().await?.to_vec();
        tokio::fs::create_dir_all(cache_path.parent().unwrap()).await?;
        // Write-then-rename so a torn write never poisons the cache.
        let tmp = cache_path.with_extension("tmp");
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &cache_path).await?;
        bytes
    };

    let content_type = sniff_image_content_type(&bytes);
    Ok(Some(
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            Body::from(bytes),
        )
            .into_response(),
    ))
}

fn sniff_image_content_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_image_types() {
        assert_eq!(sniff_image_content_type(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(
            sniff_image_content_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]),
            "image/png"
        );
        assert_eq!(sniff_image_content_type(b"RIFF0000WEBPVP8 "), "image/webp");
        assert_eq!(sniff_image_content_type(b"nonsense"), "application/octet-stream");
    }
}
