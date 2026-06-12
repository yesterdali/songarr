//! getAlbum synthesis for virtual `sga_` albums.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::subsonic::types::{Payload, SongEntry};
use crate::subsonic::{auth, error_not_found, Format};
use crate::valbum::{self, AlbumPayload, AlbumTrackPayload, VirtualAlbum};
use crate::vtrack::{self, CatalogTrack};
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

    if !valbum::is_virtual_album_id(&id) {
        if !matches!(format, Format::Json) {
            return passthrough::handler(State(state), req).await;
        }
        return match repair_real_album_artwork_json(&state, req).await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(%error, "real album artwork repair failed");
                (axum::http::StatusCode::BAD_GATEWAY, "upstream unavailable").into_response()
            }
        };
    }
    if !state.config.artist_expansion.enabled {
        return error_not_found(&state.envelope().await, format);
    }

    match synthesize_album(&state, &id).await {
        Ok(Some((album, songs))) => state
            .envelope()
            .await
            .render_ok(format, Some(album_payload(album, songs))),
        Ok(None) => error_not_found(&state.envelope().await, format),
        Err(error) => {
            tracing::error!(%error, id, "virtual album synthesis failed");
            state
                .envelope()
                .await
                .render_error(format, 0, "internal error")
        }
    }
}

async fn repair_real_album_artwork_json(
    state: &AppState,
    req: Request,
) -> anyhow::Result<Response> {
    let (status, mut headers, body) = passthrough::fetch_upstream_identity(state, req).await?;
    let body_text = match std::str::from_utf8(&body) {
        Ok(text) if status.is_success() => text,
        _ => return Ok(super::search::raw_response(status, headers, body.to_vec())),
    };
    let mut value: serde_json::Value = match serde_json::from_str(body_text) {
        Ok(value) => value,
        Err(_) => return Ok(super::search::raw_response(status, headers, body.to_vec())),
    };
    if value["subsonic-response"]["status"].as_str() != Some("ok") {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    }

    let Some(album) = value["subsonic-response"]["album"].as_object_mut() else {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    };
    let songs = album.get_mut("song").and_then(|song| song.as_array_mut());
    let Some(songs) = songs else {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    };

    let mut first_repaired_cover: Option<String> = None;
    let mut repaired_count = 0usize;
    for song in songs.iter_mut() {
        let Some(real_id) = song.get("id").and_then(|id| id.as_str()) else {
            continue;
        };
        let Ok(Some(track)) = crate::vtrack::get_by_real_subsonic_id(&state.db, real_id).await
        else {
            continue;
        };
        if track.artwork_url.is_none() {
            continue;
        }
        song["coverArt"] = serde_json::json!(track.id);
        first_repaired_cover.get_or_insert_with(|| track.id.clone());
        repaired_count += 1;
    }

    if songs.len() == 1 {
        if let Some(cover) = first_repaired_cover {
            album["coverArt"] = serde_json::json!(cover);
        }
    }
    if repaired_count > 0 {
        tracing::debug!(repaired_count, "repaired imported album artwork");
    }

    let new_body = serde_json::to_vec(&value)?;
    headers.remove(CONTENT_LENGTH);
    headers.remove(CONTENT_TYPE);
    let mut response = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, Format::Json.content_type())
        .body(Body::from(new_body))?;
    response.headers_mut().extend(headers);
    Ok(response)
}

async fn synthesize_album(
    state: &AppState,
    id: &str,
) -> anyhow::Result<Option<(VirtualAlbum, Vec<(SongEntry, AlbumTrackPayload)>)>> {
    let Some(album) = valbum::get(&state.db, id).await? else {
        return Ok(None);
    };
    let payload: AlbumPayload = serde_json::from_str(&album.payload_json)?;
    let mut songs = Vec::with_capacity(payload.tracks.len());
    for track in payload.tracks {
        let catalog = CatalogTrack {
            provider: crate::catalog::deezer::PROVIDER,
            provider_track_id: track.provider_track_id.clone(),
            artist: track.artist.clone(),
            title: track.title.clone(),
            album: Some(track.album.clone()),
            duration_ms: track.duration_ms,
            isrc: track.isrc.clone(),
            artwork_url: track
                .artwork_url
                .clone()
                .or_else(|| album.artwork_url.clone()),
        };
        let track_id = vtrack::upsert(&state.db, &catalog).await?;
        let stored = vtrack::get(&state.db, &track_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("virtual track {track_id} vanished"))?;
        let mut entry = SongEntry::from_virtual(&stored, &state.config.streaming);
        entry.album = Some(album.title.clone());
        entry.cover_art = entry.cover_art.or_else(|| {
            valbum::album_artwork_url(&album)
                .as_ref()
                .map(|_| album.id.clone())
        });
        songs.push((entry, track));
    }
    Ok(Some((album, songs)))
}

pub(crate) async fn materialize_tracks(state: &AppState, id: &str) -> anyhow::Result<()> {
    let _ = synthesize_album(state, id).await?;
    Ok(())
}

fn album_payload(
    album: VirtualAlbum,
    songs: Vec<(SongEntry, AlbumTrackPayload)>,
) -> Payload<'static> {
    let duration: i64 = songs
        .iter()
        .filter_map(|(song, _)| song.duration_secs)
        .sum();
    let created = crate::vtrack::now_utc();
    let song_json: Vec<_> = songs
        .iter()
        .map(|(song, track)| song_json(song, track, &album.id))
        .collect();
    let mut json = serde_json::json!({
        "id": album.id,
        "name": album.title,
        "artist": album.artist,
        "songCount": song_json.len(),
        "duration": duration,
        "created": created,
        "song": song_json,
    });
    let album_artwork_url = valbum::album_artwork_url(&album);
    if let Some(cover) = album_artwork_url.as_ref().map(|_| album.id.clone()) {
        json["coverArt"] = serde_json::json!(cover);
    }
    if let Some(year) = album
        .release_date
        .as_deref()
        .and_then(|date| date.get(0..4))
        .and_then(|year| year.parse::<i64>().ok())
    {
        json["year"] = serde_json::json!(year);
    }

    Payload {
        key: "album",
        json,
        write_xml: Box::new(move |writer| {
            let mut album_node = BytesStart::new("album");
            album_node.push_attribute(("id", album.id.as_str()));
            album_node.push_attribute(("name", album.title.as_str()));
            album_node.push_attribute(("artist", album.artist.as_str()));
            if album_artwork_url.is_some() {
                album_node.push_attribute(("coverArt", album.id.as_str()));
            }
            album_node.push_attribute(("songCount", songs.len().to_string().as_str()));
            album_node.push_attribute(("duration", duration.to_string().as_str()));
            album_node.push_attribute(("created", created.as_str()));
            if let Some(year) = album
                .release_date
                .as_deref()
                .and_then(|date| date.get(0..4))
            {
                album_node.push_attribute(("year", year));
            }
            writer.write_event(Event::Start(album_node)).unwrap();
            for (song, track) in &songs {
                write_song_xml(writer, song, track, &album.id);
            }
            writer
                .write_event(Event::End(BytesEnd::new("album")))
                .unwrap();
        }),
    }
}

fn song_json(song: &SongEntry, track: &AlbumTrackPayload, album_id: &str) -> serde_json::Value {
    let mut json = song.to_json();
    json["albumId"] = serde_json::json!(album_id);
    if let Some(track_number) = track.track_number {
        json["track"] = serde_json::json!(track_number);
    }
    if let Some(disc_number) = track.disc_number {
        json["discNumber"] = serde_json::json!(disc_number);
    }
    json
}

fn write_song_xml(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    song: &SongEntry,
    track: &AlbumTrackPayload,
    album_id: &str,
) {
    let mut node = BytesStart::new("song");
    node.push_attribute(("id", song.id.as_str()));
    node.push_attribute(("isDir", "false"));
    node.push_attribute(("isVideo", "false"));
    node.push_attribute(("type", "music"));
    node.push_attribute(("title", song.title.as_str()));
    node.push_attribute(("artist", song.artist.as_str()));
    if let Some(album) = &song.album {
        node.push_attribute(("album", album.as_str()));
    }
    node.push_attribute(("albumId", album_id));
    if let Some(cover) = &song.cover_art {
        node.push_attribute(("coverArt", cover.as_str()));
    }
    if let Some(duration) = song.duration_secs {
        node.push_attribute(("duration", duration.to_string().as_str()));
    }
    if let Some(track_number) = track.track_number {
        node.push_attribute(("track", track_number.to_string().as_str()));
    }
    if let Some(disc_number) = track.disc_number {
        node.push_attribute(("discNumber", disc_number.to_string().as_str()));
    }
    node.push_attribute(("suffix", song.suffix));
    node.push_attribute(("contentType", song.content_type));
    node.push_attribute(("bitRate", song.bit_rate.to_string().as_str()));
    if let Some(size) = song.size {
        node.push_attribute(("size", size.to_string().as_str()));
    }
    node.push_attribute(("created", song.created.as_str()));
    writer.write_event(Event::Empty(node)).unwrap();
}
