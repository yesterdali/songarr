//! R3 synthetic discovery playlist. Clients like Feishin understand
//! playlists even when they have no visible recommendation/radio UI.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::proxy::similar::SeedTrack;
use crate::subsonic::types::{Payload, SongEntry};
use crate::subsonic::{auth, Format};
use crate::AppState;

use super::{passthrough, search};

pub const DISCOVERY_ID: &str = "songarr_discovery";
const DISCOVERY_NAME: &str = "Songarr Discovery";
/// How long a generated discovery list is reused before regenerating. Keeps
/// the frequently-polled `getPlaylists`/`getPlaylist` off the provider APIs;
/// matches the "weekly discovery" intent in songarr-recs-plan.md.
const DISCOVERY_TTL_HOURS: u32 = 6 * 24;

pub async fn get_playlists_handler(State(state): State<AppState>, req: Request) -> Response {
    if !state.config.recommendations.enabled {
        return passthrough::handler(State(state), req).await;
    }
    let (format, username) = request_format_user(&req);
    let Some(format) = format else {
        return passthrough::handler(State(state), req).await;
    };

    let upstream = passthrough::fetch_upstream_identity(&state, req).await;
    let Ok((status, mut headers, body)) = upstream else {
        tracing::error!("getPlaylists passthrough fetch failed");
        return (axum::http::StatusCode::BAD_GATEWAY, "upstream unavailable").into_response();
    };
    let Some(body_text) = ok_body_text(status, &body, format) else {
        return search::raw_response(status, headers, body.to_vec());
    };

    let summary = discovery_summary(&state, &username).await;
    let new_body = match format {
        Format::Json => inject_playlist_json(&body_text, &summary),
        Format::Xml => inject_playlist_xml(&body_text, &summary),
    };
    match new_body {
        Ok(new_body) => {
            headers.remove(CONTENT_LENGTH);
            headers.remove(CONTENT_TYPE);
            let mut response = Response::builder()
                .status(status)
                .header(CONTENT_TYPE, format.content_type())
                .body(Body::from(new_body))
                .unwrap();
            response.headers_mut().extend(headers);
            response
        }
        Err(error) => {
            tracing::warn!(%error, "getPlaylists discovery injection failed");
            search::raw_response(status, headers, body.to_vec())
        }
    }
}

pub async fn get_playlist_handler(State(state): State<AppState>, req: Request) -> Response {
    let (format, username, id) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            Format::from_query_value(params.get("f").map(|v| v.as_ref())),
            params.get("u").map(|v| v.to_string()).unwrap_or_default(),
            params.get("id").map(|v| v.to_string()).unwrap_or_default(),
        )
    };
    if id != DISCOVERY_ID || !state.config.recommendations.enabled {
        return passthrough::handler(State(state), req).await;
    }
    let format = format.unwrap_or(Format::Xml);
    let entries = match discovery_entries(&state, &username).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, username, "failed to build discovery playlist");
            Vec::new()
        }
    };
    let summary = PlaylistSummary::from_entries(&username, &entries);
    state
        .envelope()
        .await
        .render_ok(format, Some(playlist_payload(summary, entries)))
}

/// Discovery entries, served from the per-user cache when fresh. A cache miss
/// generates (the expensive provider/yt-dlp work) and stores the resulting
/// track ids; empty results are not cached, so a user who just started
/// listening isn't locked into an empty playlist for the whole TTL.
async fn discovery_entries(state: &AppState, username: &str) -> anyhow::Result<Vec<SongEntry>> {
    if let Ok(Some(ids)) =
        crate::recs::discovery_ids_get(&state.db, username, DISCOVERY_TTL_HOURS).await
    {
        return Ok(load_entries(state, &ids).await);
    }
    let entries = generate_discovery(state, username).await?;
    if !entries.is_empty() {
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        let _ = crate::recs::discovery_ids_set(&state.db, username, &ids).await;
    }
    Ok(entries)
}

/// Re-hydrate cached discovery ids into song entries. Tracks that have since
/// vanished are skipped rather than failing the whole playlist.
async fn load_entries(state: &AppState, ids: &[String]) -> Vec<SongEntry> {
    let mut entries = Vec::with_capacity(ids.len());
    for id in ids {
        if let Ok(Some(track)) = crate::vtrack::get(&state.db, id).await {
            entries.push(SongEntry::from_virtual(&track, &state.config.streaming));
        }
    }
    entries
}

async fn generate_discovery(state: &AppState, username: &str) -> anyhow::Result<Vec<SongEntry>> {
    let target = state.config.recommendations.max_results.max(1) as usize;
    let seeds = crate::recs::recent_listen_seeds(&state.db, username, 5).await?;
    let mut entries = Vec::new();
    let mut existing = Vec::new();
    for seed in seeds {
        let seed = seed_track_from_listen(state, seed).await;
        let remaining = target.saturating_sub(entries.len());
        if remaining == 0 {
            break;
        }
        let per_seed = remaining.min(6);
        match super::similar::recommended_for_seed(state, "", &seed, per_seed, &existing).await {
            Ok(mut recs) => {
                for entry in &recs {
                    existing.push(search::SongKey::new(
                        &entry.artist,
                        &entry.title,
                        entry.duration_secs,
                    ));
                }
                entries.append(&mut recs);
            }
            Err(error) => tracing::debug!(%error, "discovery seed produced no recommendations"),
        }
    }
    Ok(entries)
}

async fn seed_track_from_listen(state: &AppState, seed: crate::recs::ListenSeed) -> SeedTrack {
    if let Some(id) = &seed.subsonic_id {
        if crate::vtrack::is_virtual_id(id) {
            if let Ok(Some(track)) = crate::vtrack::get(&state.db, id).await {
                return SeedTrack::from(track);
            }
        }
    }
    let id = seed.subsonic_id.clone().unwrap_or_else(|| {
        format!(
            "listen:{}",
            crate::recs::song_key(&seed.artist, &seed.title)
        )
    });
    SeedTrack {
        provider_track_id: id.clone(),
        id,
        artist: seed.artist,
        title: seed.title,
        duration_ms: None,
        provider: "listen".into(),
        resolved_url: None,
        resolved_score: None,
        resolved_title: None,
        resolved_at_epoch: None,
    }
}

async fn discovery_summary(state: &AppState, username: &str) -> PlaylistSummary {
    match discovery_entries(state, username).await {
        Ok(entries) => PlaylistSummary::from_entries(username, &entries),
        Err(error) => {
            tracing::debug!(%error, username, "discovery summary failed");
            PlaylistSummary::empty(username)
        }
    }
}

#[derive(Debug, Clone)]
struct PlaylistSummary {
    owner: String,
    song_count: usize,
    duration: i64,
    changed: String,
}

impl PlaylistSummary {
    fn from_entries(username: &str, entries: &[SongEntry]) -> Self {
        Self {
            owner: username.to_string(),
            song_count: entries.len(),
            duration: entries.iter().filter_map(|e| e.duration_secs).sum(),
            changed: crate::vtrack::now_utc(),
        }
    }

    fn empty(username: &str) -> Self {
        Self {
            owner: username.to_string(),
            song_count: 0,
            duration: 0,
            changed: crate::vtrack::now_utc(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": DISCOVERY_ID,
            "name": DISCOVERY_NAME,
            "owner": self.owner,
            "public": false,
            "songCount": self.song_count,
            "duration": self.duration,
            "created": self.changed,
            "changed": self.changed,
            "comment": "Generated by Songarr from recent listens"
        })
    }

    fn write_xml(&self, writer: &mut quick_xml::Writer<Vec<u8>>, element: &str) {
        let mut playlist = BytesStart::new(element);
        playlist.push_attribute(("id", DISCOVERY_ID));
        playlist.push_attribute(("name", DISCOVERY_NAME));
        playlist.push_attribute(("owner", self.owner.as_str()));
        playlist.push_attribute(("public", "false"));
        playlist.push_attribute(("songCount", self.song_count.to_string().as_str()));
        playlist.push_attribute(("duration", self.duration.to_string().as_str()));
        playlist.push_attribute(("created", self.changed.as_str()));
        playlist.push_attribute(("changed", self.changed.as_str()));
        playlist.push_attribute(("comment", "Generated by Songarr from recent listens"));
        writer.write_event(Event::Empty(playlist)).unwrap();
    }

    fn write_xml_start(&self, writer: &mut quick_xml::Writer<Vec<u8>>) {
        let mut playlist = BytesStart::new("playlist");
        playlist.push_attribute(("id", DISCOVERY_ID));
        playlist.push_attribute(("name", DISCOVERY_NAME));
        playlist.push_attribute(("owner", self.owner.as_str()));
        playlist.push_attribute(("public", "false"));
        playlist.push_attribute(("songCount", self.song_count.to_string().as_str()));
        playlist.push_attribute(("duration", self.duration.to_string().as_str()));
        playlist.push_attribute(("created", self.changed.as_str()));
        playlist.push_attribute(("changed", self.changed.as_str()));
        playlist.push_attribute(("comment", "Generated by Songarr from recent listens"));
        writer.write_event(Event::Start(playlist)).unwrap();
    }
}

fn playlist_payload(summary: PlaylistSummary, entries: Vec<SongEntry>) -> Payload<'static> {
    let mut json = summary.to_json();
    json["entry"] = serde_json::json!(entries.iter().map(SongEntry::to_json).collect::<Vec<_>>());
    Payload {
        key: "playlist",
        json,
        write_xml: Box::new(move |writer| {
            summary.write_xml_start(writer);
            for entry in &entries {
                entry.write_xml(writer, "entry");
            }
            writer
                .write_event(Event::End(BytesEnd::new("playlist")))
                .unwrap();
        }),
    }
}

fn inject_playlist_json(body: &str, summary: &PlaylistSummary) -> anyhow::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(body)?;
    let envelope = value
        .get_mut("subsonic-response")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("missing subsonic-response envelope"))?;
    let playlists = envelope
        .entry("playlists")
        .or_insert_with(|| serde_json::json!({}));
    let list = playlists
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("playlists is not an object"))?
        .entry("playlist")
        .or_insert_with(|| serde_json::json!([]));
    let list = list
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("playlist is not an array"))?;
    list.retain(|p| p["id"] != DISCOVERY_ID);
    list.push(summary.to_json());
    Ok(value.to_string())
}

fn inject_playlist_xml(body: &str, summary: &PlaylistSummary) -> anyhow::Result<String> {
    let mut reader = quick_xml::Reader::from_str(body);
    let mut writer = quick_xml::Writer::new(Vec::new());
    let mut injected = false;

    loop {
        match reader.read_event()? {
            Event::Empty(e) if e.local_name().as_ref() == b"playlists" => {
                let owned = e.into_owned();
                writer.write_event(Event::Start(owned.clone()))?;
                summary.write_xml(&mut writer, "playlist");
                writer.write_event(Event::End(BytesEnd::new("playlists")))?;
                injected = true;
            }
            Event::End(e) if e.local_name().as_ref() == b"playlists" => {
                summary.write_xml(&mut writer, "playlist");
                writer.write_event(Event::End(e))?;
                injected = true;
            }
            Event::End(e) if e.local_name().as_ref() == b"subsonic-response" && !injected => {
                writer.write_event(Event::Start(BytesStart::new("playlists")))?;
                summary.write_xml(&mut writer, "playlist");
                writer.write_event(Event::End(BytesEnd::new("playlists")))?;
                writer.write_event(Event::End(e))?;
                injected = true;
            }
            Event::Eof => break,
            event => writer.write_event(event)?,
        }
    }
    Ok(String::from_utf8(writer.into_inner())?)
}

fn request_format_user(req: &Request) -> (Option<Format>, String) {
    let params = auth::query_params(req.uri().query().unwrap_or(""));
    (
        Format::from_query_value(params.get("f").map(|v| v.as_ref())),
        params.get("u").map(|v| v.to_string()).unwrap_or_default(),
    )
}

fn ok_body_text(status: axum::http::StatusCode, body: &[u8], format: Format) -> Option<String> {
    std::str::from_utf8(body)
        .ok()
        .filter(|text| status.is_success() && search::is_ok_response(text, format))
        .map(str::to_string)
}
