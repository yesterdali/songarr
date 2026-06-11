//! getCoverArt for virtual ids: fetch provider artwork once, cache on disk,
//! serve from cache thereafter. Real ids pass through.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::subsonic::{auth, error_not_found, Format};
use crate::valbum;
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

    if !vtrack::is_virtual_id(&id) && !valbum::is_virtual_album_id(&id) {
        return passthrough::handler(State(state), req).await;
    }

    if valbum::is_virtual_album_id(&id) {
        let album = match valbum::get(&state.db, &id).await {
            Ok(Some(album)) => album,
            Ok(None) => return error_not_found(&state.envelope().await, format),
            Err(error) => {
                tracing::error!(%error, id, "getCoverArt virtual album db lookup failed");
                return error_not_found(&state.envelope().await, format);
            }
        };
        let artwork_url = valbum::album_artwork_url(&album);
        return match serve_cached_artwork_url(&state, &album.id, artwork_url.as_deref()).await {
            Ok(Some(response)) => response,
            Ok(None) => error_not_found(&state.envelope().await, format),
            Err(error) => {
                tracing::warn!(%error, id, "virtual album cover art fetch failed");
                error_not_found(&state.envelope().await, format)
            }
        };
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
    serve_cached_artwork_url(state, &track.id, track.artwork_url.as_deref()).await
}

async fn serve_cached_artwork_url(
    state: &AppState,
    cache_key: &str,
    artwork_url: Option<&str>,
) -> anyhow::Result<Option<Response>> {
    let bytes = match cached_or_fetch_artwork_url(state, cache_key, artwork_url).await? {
        Some(bytes) => bytes,
        None => return Ok(None),
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

pub(crate) async fn cache_artwork_url(
    state: &AppState,
    cache_key: &str,
    artwork_url: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    cached_or_fetch_artwork_url(state, cache_key, artwork_url)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no artwork url"))
}

async fn cached_or_fetch_artwork_url(
    state: &AppState,
    cache_key: &str,
    artwork_url: Option<&str>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let cache_path = state.artwork_cache_dir().join(format!("{cache_key}.img"));
    if let Ok(cached) = tokio::fs::read(&cache_path).await {
        return Ok(Some(cached));
    }
    let Some(artwork_url) = artwork_url else {
        return Ok(None);
    };
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
    Ok(Some(bytes))
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
        assert_eq!(
            sniff_image_content_type(&[0xFF, 0xD8, 0xFF, 0xE0]),
            "image/jpeg"
        );
        assert_eq!(
            sniff_image_content_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]),
            "image/png"
        );
        assert_eq!(sniff_image_content_type(b"RIFF0000WEBPVP8 "), "image/webp");
        assert_eq!(
            sniff_image_content_type(b"nonsense"),
            "application/octet-stream"
        );
    }
}
