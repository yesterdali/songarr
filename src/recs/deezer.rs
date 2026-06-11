//! Deezer recommendation voter. It is metadata-clean and no-auth, so R2 uses
//! it both as a source and as a lightweight canonicalizer.

use crate::vtrack::CatalogTrack;

use super::RecCandidate;

pub async fn similar_for_track(
    http: &reqwest::Client,
    api_base: &str,
    artist: &str,
    title: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let tracks = crate::catalog::deezer::top_tracks(
        http,
        api_base,
        artist,
        (limit as u32).saturating_add(4),
    )
    .await?;
    Ok(tracks
        .into_iter()
        .filter(|track| !same_title(&track.title, title))
        .take(limit)
        .map(from_catalog)
        .collect())
}

pub async fn top_songs(
    http: &reqwest::Client,
    api_base: &str,
    artist: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    Ok(
        crate::catalog::deezer::top_tracks(http, api_base, artist, limit as u32)
            .await?
            .into_iter()
            .map(from_catalog)
            .collect(),
    )
}

pub async fn canonicalize(
    http: &reqwest::Client,
    api_base: &str,
    candidate: &RecCandidate,
) -> anyhow::Result<Option<RecCandidate>> {
    let query = format!("{} {}", candidate.artist, candidate.title);
    let tracks = crate::catalog::deezer::search(http, api_base, &query, 5).await?;
    let wanted = super::song_key(&candidate.artist, &candidate.title);
    Ok(tracks
        .into_iter()
        .find(|track| super::song_key(&track.artist, &track.title) == wanted)
        .map(from_catalog))
}

fn from_catalog(track: CatalogTrack) -> RecCandidate {
    RecCandidate {
        provider: Some(track.provider.into()),
        provider_track_id: Some(track.provider_track_id),
        artist: track.artist,
        title: track.title,
        album: track.album,
        duration_ms: track.duration_ms,
        isrc: track.isrc,
        artwork_url: track.artwork_url,
        video_id: None,
    }
}

fn same_title(a: &str, b: &str) -> bool {
    super::normalize(a) == super::normalize(b)
}
