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
        let original_query = req.uri().query().unwrap_or("").to_string();
        match repaired_imported_artwork(&state, &original_query, &id).await {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(%error, id, "imported artwork repair failed");
            }
        }
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

    // Imported tracks usually have weak Navidrome album art, but Songarr still
    // has the original provider artwork. Prefer that when present; otherwise
    // defer to Navidrome's art for the real id.
    if track.artwork_url.is_none() {
        let Some(real_id) = track.real_subsonic_id.clone() else {
            return error_not_found(&state.envelope().await, format);
        };
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

/// Imported files often carry no embedded art, so Navidrome serves its
/// placeholder for them. Songarr still knows the provider artwork: map the
/// real id back to its virtual track — directly for song ids, via the
/// album's songs for `al-…` ids — and serve that instead. `Ok(None)` means
/// the id has no imported tracks behind it and should pass through.
async fn repaired_imported_artwork(
    state: &AppState,
    original_query: &str,
    id: &str,
) -> anyhow::Result<Option<Response>> {
    if let Some(track) = vtrack::get_by_real_subsonic_id(&state.db, id).await? {
        if track.artwork_url.is_some() {
            return serve_cached_artwork(state, &track).await;
        }
    }

    let Some(album_id) = id
        .strip_prefix("al-")
        .and_then(|rest| rest.split('_').next())
    else {
        return Ok(None);
    };
    if album_id.is_empty() || !album_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Ok(None);
    }

    // Repaired before? The disk cache answers without touching upstream.
    let cache_key = format!("alrepair_{album_id}");
    if let Some(response) = serve_cached_artwork_url(state, &cache_key, None).await? {
        return Ok(Some(response));
    }

    // Ask Navidrome for the album's songs (with the caller's auth) and use
    // the first imported track that still has provider artwork.
    let query = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in url::form_urlencoded::parse(original_query.as_bytes()) {
            if key != "id" && key != "f" && key != "size" {
                serializer.append_pair(&key, &value);
            }
        }
        serializer.append_pair("id", album_id);
        serializer.append_pair("f", "json");
        serializer.finish()
    };
    let album_req = Request::builder()
        .uri(format!("/rest/getAlbum?{query}"))
        .body(Body::empty())?;
    let (status, _headers, body) = passthrough::fetch_upstream_identity(state, album_req).await?;
    if !status.is_success() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_slice(&body)?;
    if value["subsonic-response"]["status"].as_str() != Some("ok") {
        return Ok(None);
    }
    let Some(songs) = value["subsonic-response"]["album"]["song"].as_array() else {
        return Ok(None);
    };
    for song in songs.iter().take(30) {
        let Some(real_id) = song["id"].as_str() else {
            continue;
        };
        let Some(track) = vtrack::get_by_real_subsonic_id(&state.db, real_id).await? else {
            continue;
        };
        let Some(artwork_url) = track.artwork_url else {
            continue;
        };
        return serve_cached_artwork_url(state, &cache_key, Some(&artwork_url)).await;
    }
    Ok(None)
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
