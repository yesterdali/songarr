//! getArtist enrichment: append provider albums as virtual `sga_` albums.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::catalog::deezer::{self, CatalogAlbumCache};
use crate::subsonic::{auth, Format};
use crate::valbum;
use crate::AppState;

use super::{passthrough, search};

#[derive(Debug, Clone)]
struct ArtistContext {
    id: String,
    name: String,
    local_album_keys: Vec<String>,
}

#[derive(Debug, Clone)]
struct AlbumSummaryEntry {
    id: String,
    name: String,
    artist: String,
    artist_id: String,
    cover_art: Option<String>,
    song_count: i64,
    duration: i64,
    created: String,
    year: Option<i64>,
    artwork_url: Option<String>,
}

pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let (format, username) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            Format::from_query_value(params.get("f").map(|v| v.as_ref())),
            params.get("u").map(|v| v.to_string()).unwrap_or_default(),
        )
    };

    let Some(format) = format else {
        return passthrough::handler(State(state), req).await;
    };
    if !state.config.artist_expansion.enabled
        || state
            .config
            .users
            .deny
            .iter()
            .any(|denied| denied == &username)
    {
        return passthrough::handler(State(state), req).await;
    }

    match intercepted(&state, req, format).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "getArtist enrichment failed");
            (axum::http::StatusCode::BAD_GATEWAY, "upstream unavailable").into_response()
        }
    }
}

pub(crate) fn spawn_prewarm(state: AppState, artist: String) {
    if !state.config.artist_expansion.enabled || artist.trim().is_empty() {
        return;
    }
    tokio::spawn(async move {
        let artist = artist.trim().to_string();
        let key = deezer::artist_key(&artist);
        {
            let mut inflight = state.artist_prewarm_inflight.lock().await;
            if !inflight.insert(key.clone()) {
                return;
            }
        }

        let result = prewarm_artist(&state, &artist).await;
        {
            let mut inflight = state.artist_prewarm_inflight.lock().await;
            inflight.remove(&key);
        }
        match result {
            Ok(count) => tracing::debug!(artist, albums = count, "artist expansion prewarmed"),
            Err(error) => tracing::debug!(%error, artist, "artist expansion prewarm failed"),
        }
    });
}

async fn prewarm_artist(state: &AppState, artist: &str) -> anyhow::Result<usize> {
    let ctx = ArtistContext {
        id: String::new(),
        name: artist.to_string(),
        local_album_keys: Vec::new(),
    };
    let entries = expand_artist(state, &ctx).await?;
    for entry in &entries {
        if let Ok(Some(album)) = valbum::get(&state.db, &entry.id).await {
            let artwork_url = valbum::album_artwork_url(&album);
            let _ =
                super::coverart::cache_artwork_url(state, &album.id, artwork_url.as_deref()).await;
            if let Err(error) = super::album::materialize_tracks(state, &album.id).await {
                tracing::debug!(%error, album = album.id, "prewarm track materialization failed");
            }
        }
    }
    Ok(entries.len())
}

async fn intercepted(state: &AppState, req: Request, format: Format) -> anyhow::Result<Response> {
    let (status, mut headers, body) = passthrough::fetch_upstream_identity(state, req).await?;
    let body_text = match std::str::from_utf8(&body) {
        Ok(text) if status.is_success() && search::is_ok_response(text, format) => text.to_owned(),
        _ => return Ok(search::raw_response(status, headers, body.to_vec())),
    };

    let Some(ctx) = extract_artist_context(&body_text, format) else {
        return Ok(search::raw_response(status, headers, body.to_vec()));
    };
    if ctx.name.trim().is_empty() {
        return Ok(search::raw_response(status, headers, body.to_vec()));
    }

    let entries = match expand_artist(state, &ctx).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, artist = ctx.name, "artist expansion failed; returning vanilla artist");
            return Ok(search::raw_response(status, headers, body.to_vec()));
        }
    };
    if entries.is_empty() {
        return Ok(search::raw_response(status, headers, body.to_vec()));
    }
    warm_album_artwork(
        state.clone(),
        entries
            .iter()
            .map(|entry| (entry.id.clone(), entry.artwork_url.clone()))
            .collect(),
    );

    let new_body = match format {
        Format::Json => inject_json(&body_text, &entries)?,
        Format::Xml => inject_xml(&body_text, &entries)?,
    };

    headers.remove(CONTENT_LENGTH);
    headers.remove(CONTENT_TYPE);
    let mut response = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, format.content_type())
        .body(Body::from(new_body))?;
    response.headers_mut().extend(headers);
    Ok(response)
}

async fn expand_artist(
    state: &AppState,
    ctx: &ArtistContext,
) -> anyhow::Result<Vec<AlbumSummaryEntry>> {
    let cfg = &state.config.artist_expansion;
    let artist_key = deezer::artist_key(&ctx.name);
    let cached = valbum::cache_get(
        &state.db,
        deezer::PROVIDER,
        &artist_key,
        cfg.cache_ttl_hours,
    )
    .await?
    .and_then(|payload| serde_json::from_str::<Vec<CatalogAlbumCache>>(&payload).ok())
    .filter(|albums| !cached_catalog_has_artwork_holes(albums));

    let catalog_albums = if let Some(albums) = cached {
        albums
    } else {
        let resolved = deezer::resolve_artist(
            &state.http,
            &state.config.external_search.api_base_deezer,
            &ctx.name,
            cfg.min_artist_match_score,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("no provider artist match for {}", ctx.name))?;
        tracing::info!(
            library_artist = ctx.name,
            provider_artist = resolved.name,
            score = resolved.score,
            "resolved artist for expansion"
        );

        let mut summaries = deezer::artist_albums(
            &state.http,
            &state.config.external_search.api_base_deezer,
            resolved.id,
            cfg.max_albums,
        )
        .await?;
        if !cfg.include_singles {
            summaries.retain(|album| album.record_type.as_deref() != Some("single"));
        }

        let mut albums = Vec::new();
        for summary in summaries.into_iter().take(cfg.max_albums as usize) {
            let mut album = deezer::catalog_album(
                &state.http,
                &state.config.external_search.api_base_deezer,
                summary.id,
                cfg.max_tracks_per_album,
            )
            .await?;
            if album.artwork_url.is_none() {
                album.artwork_url = summary.cover_xl.or(summary.cover_big);
            }
            if album.release_date.is_none() {
                album.release_date = summary.release_date;
            }
            if album.album_type.is_none() {
                album.album_type = summary.record_type;
            }
            if album.track_count.is_none() {
                album.track_count = summary.nb_tracks;
            }
            for track in &mut album.payload.tracks {
                if track.artwork_url.is_none() {
                    track.artwork_url = album.artwork_url.clone();
                }
            }
            albums.push(album);
        }

        if cfg.include_top_tracks_album {
            if let Ok(top) = deezer::top_tracks(
                &state.http,
                &state.config.external_search.api_base_deezer,
                &resolved.name,
                cfg.max_tracks_per_album,
            )
            .await
            {
                if !top.is_empty() {
                    let artwork = resolved
                        .artwork_url
                        .clone()
                        .or_else(|| top.iter().find_map(|track| track.artwork_url.clone()));
                    albums.push(CatalogAlbumCache {
                        provider_album_id: format!("artist:{}:top", resolved.id),
                        artist: resolved.name.clone(),
                        title: "Top Songs".into(),
                        album_type: Some("playlist".into()),
                        release_date: None,
                        artwork_url: artwork.clone(),
                        track_count: Some(top.len() as i64),
                        payload: valbum::AlbumPayload {
                            tracks: top
                                .into_iter()
                                .map(|track| valbum::AlbumTrackPayload {
                                    provider_track_id: track.provider_track_id,
                                    artist: track.artist,
                                    title: track.title,
                                    album: "Top Songs".into(),
                                    duration_ms: track.duration_ms,
                                    isrc: track.isrc,
                                    artwork_url: track.artwork_url.or_else(|| artwork.clone()),
                                    disc_number: Some(1),
                                    track_number: None,
                                })
                                .collect(),
                        },
                    });
                }
            }
        }

        let payload = serde_json::to_string(&albums)?;
        valbum::cache_set(&state.db, deezer::PROVIDER, &artist_key, &payload).await?;
        albums
    };

    let mut entries = Vec::new();
    for album in catalog_albums {
        if ctx
            .local_album_keys
            .iter()
            .any(|key| key == &album_key(&album.title))
        {
            continue;
        }
        let id = valbum::upsert(&state.db, &album.clone().into_new_virtual_album()).await?;
        let duration = album
            .payload
            .tracks
            .iter()
            .filter_map(|track| track.duration_ms.map(|ms| ms / 1000))
            .sum();
        let song_count = album
            .track_count
            .unwrap_or(album.payload.tracks.len() as i64)
            .max(0);
        let artwork_url = album_cover_artwork_url(&album);
        entries.push(AlbumSummaryEntry {
            id: id.clone(),
            name: album.title,
            artist: album.artist,
            artist_id: ctx.id.clone(),
            cover_art: artwork_url.as_ref().map(|_| id),
            artwork_url,
            song_count,
            duration,
            created: crate::vtrack::now_utc(),
            year: album
                .release_date
                .as_deref()
                .and_then(|date| date.get(0..4))
                .and_then(|year| year.parse().ok()),
        });
    }
    Ok(entries)
}

fn cached_catalog_has_artwork_holes(albums: &[CatalogAlbumCache]) -> bool {
    albums.iter().any(|album| {
        album.artwork_url.is_none()
            && album.album_type.as_deref() != Some("playlist")
            && !album.provider_album_id.contains(":top")
    })
}

fn album_cover_artwork_url(album: &CatalogAlbumCache) -> Option<String> {
    album.artwork_url.clone().or_else(|| {
        album
            .payload
            .tracks
            .iter()
            .find_map(|track| track.artwork_url.clone())
    })
}

pub(crate) fn warm_album_artwork(state: AppState, entries: Vec<(String, Option<String>)>) {
    if entries.is_empty() {
        return;
    }
    tokio::spawn(async move {
        for (id, artwork_url) in entries {
            let _ = super::coverart::cache_artwork_url(&state, &id, artwork_url.as_deref()).await;
        }
    });
}

fn extract_artist_context(body: &str, format: Format) -> Option<ArtistContext> {
    match format {
        Format::Json => extract_artist_context_json(body),
        Format::Xml => extract_artist_context_xml(body),
    }
}

fn extract_artist_context_json(body: &str) -> Option<ArtistContext> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let artist = &value["subsonic-response"]["artist"];
    let id = artist["id"].as_str().unwrap_or("").to_string();
    let name = artist["name"].as_str().unwrap_or("").to_string();
    let local_album_keys = artist["album"]
        .as_array()
        .map(|albums| {
            albums
                .iter()
                .filter_map(|album| {
                    album["name"]
                        .as_str()
                        .or_else(|| album["title"].as_str())
                        .map(album_key)
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ArtistContext {
        id,
        name,
        local_album_keys,
    })
}

fn extract_artist_context_xml(body: &str) -> Option<ArtistContext> {
    let mut reader = quick_xml::Reader::from_str(body);
    let mut id = String::new();
    let mut name = String::new();
    let mut albums = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"artist" => {
                for attr in e.attributes().flatten() {
                    let value = attr.unescape_value().ok()?.into_owned();
                    match attr.key.as_ref() {
                        b"id" => id = value,
                        b"name" => name = value,
                        _ => {}
                    }
                }
            }
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"album" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"name" || attr.key.as_ref() == b"title" {
                        albums.push(album_key(&attr.unescape_value().ok()?));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    Some(ArtistContext {
        id,
        name,
        local_album_keys: albums,
    })
}

fn inject_json(body: &str, entries: &[AlbumSummaryEntry]) -> anyhow::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(body)?;
    let artist = value
        .get_mut("subsonic-response")
        .and_then(|v| v.get_mut("artist"))
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("missing artist object"))?;
    let albums = artist
        .entry("album")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("artist.album is not an array"))?;
    albums.extend(entries.iter().map(AlbumSummaryEntry::to_json));
    artist["albumCount"] = serde_json::json!(albums.len());
    Ok(value.to_string())
}

fn inject_xml(body: &str, entries: &[AlbumSummaryEntry]) -> anyhow::Result<String> {
    let mut reader = quick_xml::Reader::from_str(body);
    let mut writer = quick_xml::Writer::new(Vec::new());
    let mut injected = false;

    loop {
        match reader.read_event()? {
            Event::Empty(e) if e.local_name().as_ref() == b"artist" => {
                let owned = e.into_owned();
                writer.write_event(Event::Start(owned))?;
                for entry in entries {
                    entry.write_xml(&mut writer);
                }
                writer.write_event(Event::End(BytesEnd::new("artist")))?;
                injected = true;
            }
            Event::End(e) if e.local_name().as_ref() == b"artist" => {
                for entry in entries {
                    entry.write_xml(&mut writer);
                }
                writer.write_event(Event::End(e))?;
                injected = true;
            }
            Event::End(e) if e.local_name().as_ref() == b"subsonic-response" && !injected => {
                writer.write_event(Event::End(e))?;
                injected = true;
            }
            Event::Eof => break,
            event => writer.write_event(event)?,
        }
    }
    Ok(String::from_utf8(writer.into_inner())?)
}

impl AlbumSummaryEntry {
    fn to_json(&self) -> serde_json::Value {
        let mut album = serde_json::json!({
            "id": self.id,
            "name": self.name,
            "artist": self.artist,
            "artistId": self.artist_id,
            "songCount": self.song_count,
            "duration": self.duration,
            "created": self.created,
        });
        if let Some(cover) = &self.cover_art {
            album["coverArt"] = serde_json::json!(cover);
        }
        if let Some(year) = self.year {
            album["year"] = serde_json::json!(year);
        }
        album
    }

    fn write_xml(&self, writer: &mut quick_xml::Writer<Vec<u8>>) {
        let mut album = BytesStart::new("album");
        album.push_attribute(("id", self.id.as_str()));
        album.push_attribute(("name", self.name.as_str()));
        album.push_attribute(("artist", self.artist.as_str()));
        album.push_attribute(("artistId", self.artist_id.as_str()));
        if let Some(cover) = &self.cover_art {
            album.push_attribute(("coverArt", cover.as_str()));
        }
        album.push_attribute(("songCount", self.song_count.to_string().as_str()));
        album.push_attribute(("duration", self.duration.to_string().as_str()));
        album.push_attribute(("created", self.created.as_str()));
        if let Some(year) = self.year {
            album.push_attribute(("year", year.to_string().as_str()));
        }
        writer.write_event(Event::Empty(album)).unwrap();
    }
}

fn album_key(title: &str) -> String {
    deunicode::deunicode(title)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_injection_appends_virtual_album_and_updates_count() {
        let body = r#"{"subsonic-response":{"status":"ok","artist":{"id":"ar1","name":"Artist","album":[{"id":"al1","name":"Local"}],"albumCount":1}}}"#;
        let out = inject_json(body, &[entry("sga_x", "Remote")]).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let albums = value["subsonic-response"]["artist"]["album"]
            .as_array()
            .unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[1]["id"], "sga_x");
        assert_eq!(value["subsonic-response"]["artist"]["albumCount"], 2);
    }

    #[test]
    fn xml_injection_appends_before_artist_close() {
        let body = r#"<subsonic-response status="ok"><artist id="ar1" name="Artist"><album id="al1" name="Local"/></artist></subsonic-response>"#;
        let out = inject_xml(body, &[entry("sga_x", "Remote")]).unwrap();
        assert!(out.contains(r#"<album id="sga_x" name="Remote""#), "{out}");
        assert!(out.find("al1").unwrap() < out.find("sga_x").unwrap());
    }

    #[test]
    fn cached_catalog_with_missing_regular_album_art_is_refetched() {
        let albums = vec![
            CatalogAlbumCache {
                provider_album_id: "201".into(),
                artist: "Artist".into(),
                title: "Album".into(),
                album_type: Some("album".into()),
                release_date: None,
                artwork_url: None,
                track_count: Some(1),
                payload: valbum::AlbumPayload { tracks: Vec::new() },
            },
            CatalogAlbumCache {
                provider_album_id: "artist:100:top".into(),
                artist: "Artist".into(),
                title: "Top Songs".into(),
                album_type: Some("playlist".into()),
                release_date: None,
                artwork_url: None,
                track_count: Some(0),
                payload: valbum::AlbumPayload { tracks: Vec::new() },
            },
        ];
        assert!(cached_catalog_has_artwork_holes(&albums));
        assert!(!cached_catalog_has_artwork_holes(&albums[1..]));
    }

    #[test]
    fn album_cover_falls_back_to_first_track_artwork() {
        let album = CatalogAlbumCache {
            provider_album_id: "artist:100:top".into(),
            artist: "Artist".into(),
            title: "Top Songs".into(),
            album_type: Some("playlist".into()),
            release_date: None,
            artwork_url: None,
            track_count: Some(1),
            payload: valbum::AlbumPayload {
                tracks: vec![valbum::AlbumTrackPayload {
                    provider_track_id: "t1".into(),
                    artist: "Artist".into(),
                    title: "Song".into(),
                    album: "Top Songs".into(),
                    duration_ms: Some(123_000),
                    isrc: None,
                    artwork_url: Some("https://example.com/song.jpg".into()),
                    disc_number: Some(1),
                    track_number: Some(1),
                }],
            },
        };
        assert_eq!(
            album_cover_artwork_url(&album).as_deref(),
            Some("https://example.com/song.jpg")
        );
    }

    fn entry(id: &str, name: &str) -> AlbumSummaryEntry {
        AlbumSummaryEntry {
            id: id.into(),
            name: name.into(),
            artist: "Artist".into(),
            artist_id: "ar1".into(),
            cover_art: Some(id.into()),
            song_count: 1,
            duration: 100,
            created: "2026-01-01T00:00:00Z".into(),
            year: Some(2026),
            artwork_url: Some("https://example.com/c.jpg".into()),
        }
    }
}
