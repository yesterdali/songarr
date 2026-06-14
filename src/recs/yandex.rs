//! Yandex Music recommendation source.
//!
//! V1 uses Yandex's personalized feed as a pool of taste candidates. The
//! existing merge/dedup layer handles local-library preference and anti-repeat.

use crate::recs::RecCandidate;

pub async fn wave(
    config: &crate::config::Yandex,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let tracks = crate::yandex::wave(config, limit).await?;
    Ok(tracks
        .into_iter()
        .map(|track| RecCandidate {
            artist: track.artist,
            title: track.title,
            album: track.album,
            duration_ms: track.duration_ms,
            isrc: track.isrc,
            artwork_url: track.artwork_url,
            provider: Some(crate::yandex::PROVIDER.into()),
            provider_track_id: Some(track.track_id),
            video_id: None,
        })
        .collect())
}

pub async fn search_as_recs(
    config: &crate::config::Yandex,
    artist: &str,
    title: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let query = format!("{artist} {title}");
    let tracks = crate::yandex::search(config, &query, limit).await?;
    Ok(tracks
        .into_iter()
        .map(|track| RecCandidate {
            artist: track.artist,
            title: track.title,
            album: track.album,
            duration_ms: track.duration_ms,
            isrc: track.isrc,
            artwork_url: track.artwork_url,
            provider: Some(crate::yandex::PROVIDER.into()),
            provider_track_id: Some(track.track_id),
            video_id: None,
        })
        .collect())
}
