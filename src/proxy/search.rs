//! search3/search2 interception: forward to Navidrome, then append external
//! catalog hits as virtual songs — deduplicated against Navidrome's own
//! results and already-imported tracks (normalized artist+title, ±3s).

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::subsonic::types::SongEntry;
use crate::subsonic::{auth, Format};
use crate::vtrack::CatalogTrack;
use crate::AppState;

use super::passthrough;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Search2,
    Search3,
}

impl SearchKind {
    fn result_key(self) -> &'static str {
        match self {
            SearchKind::Search2 => "searchResult2",
            SearchKind::Search3 => "searchResult3",
        }
    }
}

pub async fn search2_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, SearchKind::Search2).await
}

pub async fn search3_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, SearchKind::Search3).await
}

async fn handle(state: AppState, req: Request, kind: SearchKind) -> Response {
    let (format, username, search_query) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            Format::from_query_value(params.get("f").map(|v| v.as_ref())),
            params.get("u").map(|v| v.to_string()).unwrap_or_default(),
            params
                .get("query")
                .map(|v| v.to_string())
                .unwrap_or_default(),
        )
    };
    // Symfonium and others probe with `""` (quoted empty) for "browse all".
    let effective_query = search_query.trim_matches('"').trim().to_string();

    let cfg = &state.config.external_search;
    let intercept = cfg.enabled
        && format.is_some()
        && effective_query.len() >= cfg.min_query_len as usize
        && !state
            .config
            .users
            .deny
            .iter()
            .any(|denied| denied == &username);

    if !intercept {
        return passthrough::handler(State(state), req).await;
    }
    let format = format.unwrap();

    match intercepted(&state, req, kind, format, &effective_query).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "search interception failed");
            // Never break search over injection trouble: a fresh passthrough
            // is impossible here (request consumed), so surface a subsonic
            // error only if upstream itself failed; this path is upstream
            // failure, mirror passthrough's behavior.
            (axum::http::StatusCode::BAD_GATEWAY, "upstream unavailable").into_response()
        }
    }
}

async fn intercepted(
    state: &AppState,
    req: Request,
    kind: SearchKind,
    format: Format,
    search_query: &str,
) -> anyhow::Result<Response> {
    let (status, mut headers, body) = passthrough::fetch_upstream_identity(state, req).await?;

    // Only touch a healthy, OK-status subsonic response; pass anything else
    // (errors, unexpected content) along unmodified.
    let body_text = match std::str::from_utf8(&body) {
        Ok(text) if status.is_success() && is_ok_response(text, format) => text.to_owned(),
        _ => {
            return Ok(raw_response(status, headers, body.to_vec()));
        }
    };

    let catalog = match crate::catalog::search(
        &state.http,
        &state.config.external_search,
        search_query,
    )
    .await
    {
        Ok(results) => results,
        Err(error) => {
            tracing::warn!(%error, query = search_query, "catalog search failed; returning vanilla results");
            return Ok(raw_response(status, headers, body.into()));
        }
    };

    let existing = match format {
        Format::Json => existing_songs_json(&body_text, kind.result_key()),
        Format::Xml => existing_songs_xml(&body_text),
    };
    let imported = crate::vtrack::imported(&state.db).await.unwrap_or_default();
    let imported_keys: Vec<SongKey> = imported
        .iter()
        .map(|t| SongKey::new(&t.artist, &t.title, t.duration_ms.map(|ms| ms / 1000)))
        .collect();

    let mut chosen: Vec<CatalogTrack> = Vec::new();
    for track in catalog {
        let key = SongKey::new(
            &track.artist,
            &track.title,
            track.duration_ms.map(|ms| ms / 1000),
        );
        let duplicate = existing.iter().any(|e| e.matches(&key))
            || imported_keys.iter().any(|e| e.matches(&key))
            || chosen.iter().any(|c| {
                SongKey::new(&c.artist, &c.title, c.duration_ms.map(|ms| ms / 1000)).matches(&key)
            });
        if duplicate {
            continue;
        }
        chosen.push(track);
        if chosen.len() >= state.config.external_search.max_results as usize {
            break;
        }
    }

    if chosen.is_empty() {
        return Ok(raw_response(status, headers, body.into()));
    }

    let mut entries = Vec::with_capacity(chosen.len());
    for track in &chosen {
        let id = crate::vtrack::upsert(&state.db, track).await?;
        let stored = crate::vtrack::get(&state.db, &id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("virtual track {id} vanished"))?;
        entries.push(super::song_entry_with_repaired_artwork(state, stored).await);
    }
    tracing::info!(
        query = search_query,
        injected = entries.len(),
        "appended virtual results to {}",
        kind.result_key()
    );

    // The user is now looking at these results; resolve YouTube sources in
    // the background so the likely play starts without the search latency.
    let prefetch_ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    tokio::spawn(crate::resolve::prefetch(state.clone(), prefetch_ids));

    let new_body = match format {
        Format::Json => inject_json(&body_text, kind.result_key(), &entries)?,
        Format::Xml => inject_xml(&body_text, kind.result_key(), &entries)?,
    };

    headers.remove(CONTENT_LENGTH); // recomputed by hyper from the new body
    headers.remove(CONTENT_TYPE); // keep upstream's; re-add below unchanged
    let content_type = format.content_type();
    let mut response = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from(new_body))?;
    response.headers_mut().extend(headers);
    Ok(response)
}

pub(crate) fn raw_response(
    status: axum::http::StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
) -> Response {
    let mut response = Response::builder()
        .status(status)
        .body(Body::from(body))
        .unwrap();
    *response.headers_mut() = headers;
    response
}

pub(crate) fn is_ok_response(body: &str, format: Format) -> bool {
    match format {
        Format::Json => serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.get("subsonic-response")
                    .and_then(|r| r.get("status"))
                    .and_then(|s| s.as_str())
                    .map(|s| s == "ok")
            })
            .unwrap_or(false),
        Format::Xml => {
            let mut reader = quick_xml::Reader::from_str(body);
            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e))
                        if e.local_name().as_ref() == b"subsonic-response" =>
                    {
                        return e.attributes().flatten().any(|attr| {
                            attr.key.as_ref() == b"status"
                                && attr
                                    .unescape_value()
                                    .map(|value| value.as_ref() == "ok")
                                    .unwrap_or(false)
                        });
                    }
                    Ok(Event::Eof) => return false,
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
        }
    }
}

// ---- Dedup keys ----

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SongKey {
    artist: String,
    title: String,
    duration_secs: Option<i64>,
}

impl SongKey {
    pub(crate) fn new(artist: &str, title: &str, duration_secs: Option<i64>) -> Self {
        Self {
            artist: crate::recs::artist_key(artist),
            title: crate::recs::title_key(title),
            duration_secs,
        }
    }

    /// Same normalized artist+title; durations only veto when both known
    /// and more than 3s apart.
    pub(crate) fn matches(&self, other: &Self) -> bool {
        if self.artist != other.artist || self.title != other.title {
            return false;
        }
        match (self.duration_secs, other.duration_secs) {
            (Some(a), Some(b)) => (a - b).abs() <= 3,
            _ => true,
        }
    }
}

/// Lowercased, transliterated to ASCII, alphanumerics only.
#[cfg(test)]
fn normalize(value: &str) -> String {
    crate::recs::normalize(value)
}

// ---- Existing-song extraction ----

pub(crate) fn existing_songs_json(body: &str, result_key: &str) -> Vec<SongKey> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let songs = &value["subsonic-response"][result_key]["song"];
    songs
        .as_array()
        .map(|songs| {
            songs
                .iter()
                .map(|s| {
                    SongKey::new(
                        s["artist"].as_str().unwrap_or(""),
                        s["title"].as_str().unwrap_or(""),
                        s["duration"].as_i64(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn existing_songs_xml(body: &str) -> Vec<SongKey> {
    let mut reader = quick_xml::Reader::from_str(body);
    let mut keys = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"song" => {
                let mut artist = String::new();
                let mut title = String::new();
                let mut duration = None;
                for attr in e.attributes().flatten() {
                    let value = attr.unescape_value().unwrap_or_default().into_owned();
                    match attr.key.as_ref() {
                        b"artist" => artist = value,
                        b"title" => title = value,
                        b"duration" => duration = value.parse().ok(),
                        _ => {}
                    }
                }
                keys.push(SongKey::new(&artist, &title, duration));
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    keys
}

// ---- Injection ----

pub(crate) fn inject_json(
    body: &str,
    result_key: &str,
    entries: &[SongEntry],
) -> anyhow::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(body)?;
    let envelope = value
        .get_mut("subsonic-response")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("missing subsonic-response envelope"))?;
    let result = envelope
        .entry(result_key)
        .or_insert_with(|| serde_json::json!({}));
    let songs = result
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{result_key} is not an object"))?
        .entry("song")
        .or_insert_with(|| serde_json::json!([]));
    let songs = songs
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("song is not an array"))?;
    songs.extend(entries.iter().map(SongEntry::to_json));
    Ok(value.to_string())
}

/// Append `<song …/>` elements inside `<searchResultN>`, handling the
/// normal, self-closing, and absent element cases. All untouched events are
/// re-emitted as-is.
pub(crate) fn inject_xml(
    body: &str,
    result_key: &str,
    entries: &[SongEntry],
) -> anyhow::Result<String> {
    let mut reader = quick_xml::Reader::from_str(body);
    let mut writer = quick_xml::Writer::new(Vec::new());
    let mut injected = false;

    loop {
        match reader.read_event()? {
            Event::Empty(e) if e.local_name().as_ref() == result_key.as_bytes() => {
                // <searchResult3/> → open, fill, close.
                let owned = e.into_owned();
                writer.write_event(Event::Start(owned.clone()))?;
                for entry in entries {
                    entry.write_xml(&mut writer, "song");
                }
                writer.write_event(Event::End(BytesEnd::new(result_key)))?;
                injected = true;
            }
            Event::End(e) if e.local_name().as_ref() == result_key.as_bytes() => {
                for entry in entries {
                    entry.write_xml(&mut writer, "song");
                }
                writer.write_event(Event::End(e))?;
                injected = true;
            }
            Event::End(e) if e.local_name().as_ref() == b"subsonic-response" && !injected => {
                // No searchResultN element at all: create it.
                writer.write_event(Event::Start(BytesStart::new(result_key)))?;
                for entry in entries {
                    entry.write_xml(&mut writer, "song");
                }
                writer.write_event(Event::End(BytesEnd::new(result_key)))?;
                writer.write_event(Event::End(e))?;
                injected = true;
            }
            Event::Eof => break,
            event => writer.write_event(event)?,
        }
    }
    Ok(String::from_utf8(writer.into_inner())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Streaming;
    use crate::vtrack::VirtualTrack;

    fn entry(id: &str, artist: &str, title: &str, secs: i64) -> SongEntry {
        SongEntry::from_virtual(
            &VirtualTrack {
                id: id.into(),
                provider: "deezer".into(),
                provider_track_id: "1".into(),
                artist: artist.into(),
                title: title.into(),
                album: Some("Album".into()),
                duration_ms: Some(secs * 1000),
                isrc: None,
                artwork_url: Some("https://example.com/c.jpg".into()),
                status: "virtual".into(),
                real_subsonic_id: None,
                resolved_url: None,
                resolved_score: None,
                resolved_title: None,
                resolved_at_epoch: None,
            },
            &Streaming::default(),
        )
    }

    #[test]
    fn normalize_strips_accents_case_punctuation() {
        assert_eq!(normalize("Daft Punk"), "daftpunk");
        assert_eq!(normalize("Sigur Rós"), "sigurros");
        assert_eq!(normalize("AC/DC!"), "acdc");
    }

    #[test]
    fn song_keys_match_with_duration_tolerance() {
        let a = SongKey::new("Daft Punk", "One More Time", Some(320));
        assert!(a.matches(&SongKey::new("daft punk", "One More Time!", Some(322))));
        assert!(a.matches(&SongKey::new("Daft Punk", "One More Time", None)));
        assert!(!a.matches(&SongKey::new("Daft Punk", "One More Time", Some(360))));
        assert!(!a.matches(&SongKey::new("Daft Punk", "Around the World", Some(320))));
        assert!(
            SongKey::new("Molchat Doma", "Sudno", Some(130)).matches(&SongKey::new(
                "Молчат Дома",
                "Судно (Борис Рыжий)",
                Some(129)
            ))
        );
    }

    const JSON_FIXTURE: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"navidrome","serverVersion":"0.62.0 (1b46b977)","openSubsonic":true,"searchResult3":{"song":[{"id":"abc","title":"Tone 220 Hz","artist":"The Sine Waves","duration":4}]}}}"#;

    #[test]
    fn ok_response_detection_parses_json_and_xml() {
        let pretty_json = r#"{
          "subsonic-response": {
            "status": "ok",
            "version": "1.16.1"
          }
        }"#;
        assert!(is_ok_response(pretty_json, Format::Json));
        assert!(!is_ok_response(
            r#"{"subsonic-response":{"status":"failed"}}"#,
            Format::Json
        ));
        assert!(is_ok_response(
            r#"<subsonic-response xmlns="http://subsonic.org/restapi" status="ok" version="1.16.1"/>"#,
            Format::Xml
        ));
        assert!(!is_ok_response(
            r#"<subsonic-response status="failed" version="1.16.1"/>"#,
            Format::Xml
        ));
    }

    #[test]
    fn json_injection_appends_after_existing() {
        let out = inject_json(
            JSON_FIXTURE,
            "searchResult3",
            &[entry("sgr_x", "A", "B", 100)],
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let songs = v["subsonic-response"]["searchResult3"]["song"]
            .as_array()
            .unwrap();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0]["id"], "abc");
        assert_eq!(songs[1]["id"], "sgr_x");
        // Envelope untouched.
        assert_eq!(v["subsonic-response"]["serverVersion"], "0.62.0 (1b46b977)");
    }

    #[test]
    fn json_injection_creates_missing_song_array() {
        let body = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","searchResult3":{}}}"#;
        let out = inject_json(body, "searchResult3", &[entry("sgr_x", "A", "B", 100)]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["subsonic-response"]["searchResult3"]["song"][0]["id"],
            "sgr_x"
        );
    }

    const XML_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><subsonic-response xmlns="http://subsonic.org/restapi" status="ok" version="1.16.1" type="navidrome" serverVersion="0.62.0 (1b46b977)" openSubsonic="true"><searchResult3><song id="abc" title="Tone 220 Hz" artist="The Sine Waves" duration="4"></song></searchResult3></subsonic-response>"#;

    #[test]
    fn xml_injection_appends_inside_result_element() {
        let out = inject_xml(
            XML_FIXTURE,
            "searchResult3",
            &[entry("sgr_x", "A", "B", 100)],
        )
        .unwrap();
        assert!(out.contains(r#"<song id="abc""#), "{out}");
        assert!(out.contains(r#"<song id="sgr_x""#), "{out}");
        assert!(
            out.find("abc").unwrap() < out.find("sgr_x").unwrap(),
            "virtual songs must come after real ones: {out}"
        );
        assert!(
            out.contains(r#"serverVersion="0.62.0 (1b46b977)""#),
            "{out}"
        );
    }

    #[test]
    fn xml_injection_handles_self_closing_and_missing_element() {
        let self_closing = r#"<subsonic-response status="ok" version="1.16.1"><searchResult3/></subsonic-response>"#;
        let out = inject_xml(
            self_closing,
            "searchResult3",
            &[entry("sgr_x", "A", "B", 9)],
        )
        .unwrap();
        assert!(out.contains(r#"<searchResult3><song id="sgr_x""#), "{out}");
        assert!(out.contains("</searchResult3>"), "{out}");

        let missing = r#"<subsonic-response status="ok" version="1.16.1"></subsonic-response>"#;
        let out = inject_xml(missing, "searchResult3", &[entry("sgr_x", "A", "B", 9)]).unwrap();
        assert!(out.contains(r#"<searchResult3><song id="sgr_x""#), "{out}");
    }

    #[test]
    fn existing_song_extraction_both_formats() {
        let json = existing_songs_json(JSON_FIXTURE, "searchResult3");
        let xml = existing_songs_xml(XML_FIXTURE);
        assert_eq!(json.len(), 1);
        assert_eq!(json, xml);
        assert!(json[0].matches(&SongKey::new("the sine waves", "Tone 220 Hz", Some(5))));
    }
}
