//! Last.fm recommendation voter. Disabled unless an API key is configured.

use serde::Deserialize;

use super::RecCandidate;

pub async fn similar_for_track(
    http: &reqwest::Client,
    api_base: &str,
    api_key: &str,
    artist: &str,
    title: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let response: SimilarTracksResponse = http
        .get(api_base.trim_end_matches('/'))
        .query(&[
            ("method", "track.getSimilar"),
            ("artist", artist),
            ("track", title),
            ("api_key", api_key),
            ("format", "json"),
            ("limit", &limit.to_string()),
        ])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response
        .similartracks
        .track
        .into_iter()
        .filter_map(|track| track.into_candidate())
        .collect())
}

pub async fn top_songs(
    http: &reqwest::Client,
    api_base: &str,
    api_key: &str,
    artist: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let response: TopTracksResponse = http
        .get(api_base.trim_end_matches('/'))
        .query(&[
            ("method", "artist.getTopTracks"),
            ("artist", artist),
            ("api_key", api_key),
            ("format", "json"),
            ("limit", &limit.to_string()),
        ])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response
        .toptracks
        .track
        .into_iter()
        .filter_map(|track| track.into_candidate())
        .collect())
}

#[derive(Debug, Deserialize)]
struct SimilarTracksResponse {
    #[serde(default)]
    similartracks: TrackList,
}

#[derive(Debug, Deserialize)]
struct TopTracksResponse {
    #[serde(default)]
    toptracks: TrackList,
}

#[derive(Debug, Default, Deserialize)]
struct TrackList {
    #[serde(default)]
    track: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct Track {
    name: String,
    artist: ArtistField,
    #[serde(default)]
    duration: Option<String>,
}

impl Track {
    fn into_candidate(self) -> Option<RecCandidate> {
        let artist = self.artist.name();
        if artist.is_empty() || self.name.trim().is_empty() {
            return None;
        }
        let duration_ms = self
            .duration
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .map(|seconds| seconds * 1000);
        Some(RecCandidate {
            artist,
            title: self.name,
            album: None,
            duration_ms,
            isrc: None,
            artwork_url: None,
            provider: None,
            provider_track_id: None,
            video_id: None,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ArtistField {
    Object { name: String },
    Name(String),
}

impl ArtistField {
    fn name(self) -> String {
        match self {
            ArtistField::Object { name } => name,
            ArtistField::Name(name) => name,
        }
    }
}
