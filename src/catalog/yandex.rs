//! Yandex Music catalog search via the JSON helper.

use crate::vtrack::CatalogTrack;

pub async fn search(
    config: &crate::config::Yandex,
    query: &str,
    limit: u32,
) -> anyhow::Result<Vec<CatalogTrack>> {
    let tracks = crate::yandex::search(config, query, limit as usize).await?;
    Ok(tracks
        .into_iter()
        .map(|track| CatalogTrack {
            provider: crate::yandex::PROVIDER,
            provider_track_id: track.track_id,
            artist: track.artist,
            title: track.title,
            album: track.album,
            duration_ms: track.duration_ms,
            isrc: track.isrc,
            artwork_url: track.artwork_url,
        })
        .collect())
}
