//! Deezer public search API — no key required.
//! `GET https://api.deezer.com/search?q=…` returns track entries with
//! artist/title/album/duration/artwork (ISRC only on the track detail
//! endpoint, so it stays None here).

use serde::Deserialize;

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
    artist: Artist,
    #[serde(default)]
    album: Option<Album>,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Artist {
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
            artwork_url: t
                .album
                .and_then(|a| a.cover_xl.or(a.cover_big)),
        })
        .collect())
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
}
