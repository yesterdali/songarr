//! Deezer public search API — no key required.
//! `GET https://api.deezer.com/search?q=…` returns track entries with
//! artist/title/album/duration/artwork (ISRC only on the track detail
//! endpoint, so it stays None here). Artist expansion also uses the public
//! artist and album endpoints.

use serde::{Deserialize, Serialize};

use crate::valbum::{AlbumPayload, AlbumTrackPayload, NewVirtualAlbum};
use crate::vtrack::CatalogTrack;

pub const PROVIDER: &str = "deezer";

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct Track {
    id: u64,
    title: String,
    /// Seconds.
    #[serde(default)]
    duration: Option<i64>,
    artist: TrackArtist,
    #[serde(default)]
    album: Option<Album>,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct TrackArtist {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Album {
    title: String,
    #[serde(default)]
    cover_big: Option<String>,
    #[serde(default)]
    cover_xl: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArtistSearchResponse {
    #[serde(default)]
    data: Vec<ArtistHit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistHit {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub nb_fan: Option<u64>,
    #[serde(default)]
    pub picture_big: Option<String>,
    #[serde(default)]
    pub picture_xl: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedArtist {
    pub id: u64,
    pub name: String,
    pub score: u32,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArtistAlbumsResponse {
    #[serde(default)]
    data: Vec<AlbumSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumSummary {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub record_type: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub cover_big: Option<String>,
    #[serde(default)]
    pub cover_xl: Option<String>,
    #[serde(default)]
    pub nb_tracks: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AlbumDetail {
    id: u64,
    title: String,
    #[serde(default)]
    record_type: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    cover_big: Option<String>,
    #[serde(default)]
    cover_xl: Option<String>,
    #[serde(default)]
    nb_tracks: Option<i64>,
    artist: TrackArtist,
    tracks: AlbumTrackList,
}

#[derive(Debug, Deserialize)]
struct AlbumTrackList {
    #[serde(default)]
    data: Vec<AlbumTrack>,
}

#[derive(Debug, Deserialize)]
struct AlbumTrack {
    id: u64,
    title: String,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    artist: Option<TrackArtist>,
    #[serde(default)]
    disk_number: Option<i64>,
    #[serde(default)]
    track_position: Option<i64>,
    #[serde(default)]
    isrc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogAlbumCache {
    pub provider_album_id: String,
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub album_type: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub track_count: Option<i64>,
    pub payload: AlbumPayload,
}

pub async fn search(
    http: &reqwest::Client,
    api_base: &str,
    query: &str,
    limit: u32,
) -> anyhow::Result<Vec<CatalogTrack>> {
    let url = format!("{}/search", api_base.trim_end_matches('/'));
    let response: SearchResponse = http
        .get(url)
        .query(&[("q", query), ("limit", &limit.to_string())])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response
        .data
        .into_iter()
        .filter(|t| t.kind.is_empty() || t.kind == "track")
        .map(|t| CatalogTrack {
            provider: PROVIDER,
            provider_track_id: t.id.to_string(),
            artist: t.artist.name,
            title: t.title,
            album: t.album.as_ref().map(|a| a.title.clone()),
            duration_ms: t.duration.map(|s| s * 1000),
            isrc: None,
            artwork_url: t.album.and_then(|a| a.cover_xl.or(a.cover_big)),
        })
        .collect())
}

/// Top tracks for an artist via the advanced search syntax
/// (`q=artist:"NAME"` returns that artist's tracks ranked by popularity).
/// Caller still dedups/filters; we only drop obvious other-artist hits.
pub async fn top_tracks(
    http: &reqwest::Client,
    api_base: &str,
    artist: &str,
    limit: u32,
) -> anyhow::Result<Vec<CatalogTrack>> {
    let query = format!("artist:\"{}\"", artist.replace('"', ""));
    let tracks = search(http, api_base, &query, limit).await?;
    let wanted = artist.to_lowercase();
    Ok(tracks
        .into_iter()
        .filter(|t| t.artist.to_lowercase() == wanted)
        .collect())
}

pub async fn search_artists(
    http: &reqwest::Client,
    api_base: &str,
    query: &str,
    limit: u32,
) -> anyhow::Result<Vec<ArtistHit>> {
    let url = format!("{}/search/artist", api_base.trim_end_matches('/'));
    let response: ArtistSearchResponse = http
        .get(url)
        .query(&[("q", query), ("limit", &limit.to_string())])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response.data)
}

pub async fn resolve_artist(
    http: &reqwest::Client,
    api_base: &str,
    library_artist: &str,
    min_score: u32,
) -> anyhow::Result<Option<ResolvedArtist>> {
    let hits = search_artists(http, api_base, library_artist, 10).await?;
    Ok(resolve_artist_from_hits(library_artist, hits, min_score))
}

fn resolve_artist_from_hits(
    library_artist: &str,
    hits: Vec<ArtistHit>,
    min_score: u32,
) -> Option<ResolvedArtist> {
    let mut best: Option<(u32, ArtistHit)> = None;
    for hit in hits {
        let score = artist_match_score(library_artist, &hit.name);
        if best.as_ref().is_none_or(|(best_score, best_hit)| {
            score > *best_score
                || (score == *best_score && hit.nb_fan.unwrap_or(0) > best_hit.nb_fan.unwrap_or(0))
        }) {
            best = Some((score, hit));
        }
    }
    let (score, hit) = best?;
    if score < min_score {
        return None;
    }
    Some(ResolvedArtist {
        id: hit.id,
        name: hit.name,
        score,
        artwork_url: hit.picture_xl.or(hit.picture_big),
    })
}

pub fn artist_key(name: &str) -> String {
    let normalized = normalize_artist(name);
    match normalized.as_str() {
        "oksimiron" | "oxxxymiron" => "oxxxymiron".into(),
        "molchatdoma" => "molchatdoma".into(),
        "skriptonit" | "skryptonite" | "scriptonite" => "skryptonite".into(),
        "kino" => "kino".into(),
        _ => normalized,
    }
}

fn artist_match_score(library_artist: &str, candidate: &str) -> u32 {
    let library_key = artist_key(library_artist);
    let candidate_key = artist_key(candidate);
    if library_key == candidate_key {
        return 100;
    }

    let lib = normalize_artist(library_artist);
    let cand = normalize_artist(candidate);
    if lib == cand {
        return 95;
    }
    let jw = strsim::jaro_winkler(&lib, &cand);
    (jw * 90.0).round() as u32
}

fn normalize_artist(name: &str) -> String {
    deunicode::deunicode(name)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

pub async fn artist_albums(
    http: &reqwest::Client,
    api_base: &str,
    artist_id: u64,
    limit: u32,
) -> anyhow::Result<Vec<AlbumSummary>> {
    let url = format!(
        "{}/artist/{artist_id}/albums",
        api_base.trim_end_matches('/')
    );
    let response: ArtistAlbumsResponse = http
        .get(url)
        .query(&[("limit", &limit.to_string())])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response.data)
}

pub async fn catalog_album(
    http: &reqwest::Client,
    api_base: &str,
    album_id: u64,
    max_tracks: u32,
) -> anyhow::Result<CatalogAlbumCache> {
    let url = format!("{}/album/{album_id}", api_base.trim_end_matches('/'));
    let detail: AlbumDetail = http
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(detail.into_cache(max_tracks))
}

impl AlbumDetail {
    fn into_cache(self, max_tracks: u32) -> CatalogAlbumCache {
        let artwork_url = self.cover_xl.or(self.cover_big);
        let tracks = self
            .tracks
            .data
            .into_iter()
            .take(max_tracks as usize)
            .map(|track| AlbumTrackPayload {
                provider_track_id: track.id.to_string(),
                artist: track
                    .artist
                    .map(|a| a.name)
                    .unwrap_or_else(|| self.artist.name.clone()),
                title: track.title,
                album: self.title.clone(),
                duration_ms: track.duration.map(|s| s * 1000),
                isrc: track.isrc,
                artwork_url: artwork_url.clone(),
                disc_number: track.disk_number,
                track_number: track.track_position,
            })
            .collect();
        CatalogAlbumCache {
            provider_album_id: self.id.to_string(),
            artist: self.artist.name,
            title: self.title,
            album_type: self.record_type,
            release_date: self.release_date,
            artwork_url,
            track_count: self.nb_tracks,
            payload: AlbumPayload { tracks },
        }
    }
}

impl CatalogAlbumCache {
    pub fn into_new_virtual_album(self) -> NewVirtualAlbum {
        NewVirtualAlbum {
            provider: PROVIDER,
            provider_album_id: self.provider_album_id,
            artist: self.artist,
            title: self.title,
            album_type: self.album_type,
            release_date: self.release_date,
            artwork_url: self.artwork_url,
            track_count: self.track_count,
            payload: self.payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_world_shape() {
        // Trimmed from a real api.deezer.com/search response.
        let raw = r#"{
          "data": [
            {
              "id": 3135556,
              "readable": true,
              "title": "Harder, Better, Faster, Stronger",
              "title_short": "Harder, Better, Faster, Stronger",
              "link": "https://www.deezer.com/track/3135556",
              "duration": 224,
              "rank": 956167,
              "explicit_lyrics": false,
              "preview": "https://cdn-preview.dzcdn.net/x.mp3",
              "artist": {
                "id": 27,
                "name": "Daft Punk",
                "link": "https://www.deezer.com/artist/27"
              },
              "album": {
                "id": 302127,
                "title": "Discovery",
                "cover": "https://api.deezer.com/album/302127/image",
                "cover_big": "https://cdn-images.dzcdn.net/big.jpg",
                "cover_xl": "https://cdn-images.dzcdn.net/xl.jpg"
              },
              "type": "track"
            }
          ],
          "total": 1
        }"#;
        let parsed: SearchResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.data.len(), 1);
        let t = &parsed.data[0];
        assert_eq!(t.artist.name, "Daft Punk");
        assert_eq!(t.duration, Some(224));
        assert_eq!(
            t.album.as_ref().unwrap().cover_xl.as_deref(),
            Some("https://cdn-images.dzcdn.net/xl.jpg")
        );
    }

    #[test]
    fn resolves_cyrillic_library_artist_to_latin_catalog_artist() {
        let hits = vec![
            ArtistHit {
                id: 1,
                name: "Oxxxy Miron Tribute".into(),
                nb_fan: Some(999_999),
                picture_big: None,
                picture_xl: None,
            },
            ArtistHit {
                id: 2,
                name: "Oxxxymiron".into(),
                nb_fan: Some(10),
                picture_big: Some("big".into()),
                picture_xl: None,
            },
        ];
        let resolved = resolve_artist_from_hits("Оксимирон", hits, 70).unwrap();
        assert_eq!(resolved.id, 2);
        assert_eq!(resolved.score, 100);
    }

    #[test]
    fn aliases_common_cyrillic_latin_artist_pairs() {
        assert_eq!(artist_key("Молчат Дома"), artist_key("Molchat Doma"));
        assert_eq!(artist_key("Скриптонит"), artist_key("Skryptonite"));
        assert_eq!(artist_key("Кино"), artist_key("Kino"));
    }
}
