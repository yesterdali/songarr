//! getLyricsBySongId interception. Navidrome lyrics (embedded tags / .lrc)
//! win for real tracks; virtual tracks and empty real-track responses fall
//! back to Songarr's provider chain (currently LRCLIB).

use axum::body::Body;
use axum::extract::{Request, State};
use axum::response::Response;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use serde::Deserialize;

use crate::lyrics::{Lyrics, SyncedLine};
use crate::subsonic::types::Payload;
use crate::subsonic::{auth, Format};
use crate::vtrack;
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

    if vtrack::is_virtual_id(&id) {
        return virtual_lyrics(&state, &id, format).await;
    }

    match real_lyrics_with_fallback(&state, req, format, &id).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, id, "lyrics fallback failed; returning empty lyrics");
            state
                .envelope()
                .await
                .render_ok(format, Some(empty_payload()))
        }
    }
}

async fn virtual_lyrics(state: &AppState, id: &str, format: Format) -> Response {
    let envelope = state.envelope().await;
    let track = match vtrack::get(&state.db, id).await {
        Ok(Some(track)) => track,
        Ok(None) => return envelope.render_ok(format, Some(empty_payload())),
        Err(error) => {
            tracing::error!(%error, id, "getLyricsBySongId virtual track lookup failed");
            return envelope.render_ok(format, Some(empty_payload()));
        }
    };

    let lyrics = match crate::lyrics::lookup(
        state,
        &track.artist,
        &track.title,
        track.album.as_deref(),
        track.duration_ms.map(|ms| ms / 1000),
    )
    .await
    {
        Ok(lyrics) => lyrics,
        Err(error) => {
            tracing::debug!(%error, id, "virtual lyrics lookup failed");
            None
        }
    };

    envelope.render_ok(format, Some(lyrics_payload(lyrics)))
}

async fn real_lyrics_with_fallback(
    state: &AppState,
    req: Request,
    format: Format,
    id: &str,
) -> anyhow::Result<Response> {
    let original_query = req.uri().query().unwrap_or("").to_string();
    let (status, headers, body) = passthrough::fetch_upstream_identity(state, req).await?;

    let body_text = std::str::from_utf8(&body).ok();
    if !status.is_success() || body_text.is_none() {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    }
    let body_text = body_text.unwrap();
    if has_lyrics(body_text, format) {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    }
    if !is_subsonic_response(body_text, format) {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    }

    let Some(song) = fetch_song_metadata(state, &original_query, id).await? else {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    };
    let Some(artist) = song
        .artist
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    };
    let Some(title) = song
        .title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    };

    let lyrics =
        crate::lyrics::lookup(state, artist, title, song.album.as_deref(), song.duration).await?;
    if lyrics.is_none() {
        return Ok(super::search::raw_response(status, headers, body.to_vec()));
    }

    Ok(state
        .envelope()
        .await
        .render_ok(format, Some(lyrics_payload(lyrics))))
}

#[derive(Debug, Deserialize)]
struct SongEnvelope {
    #[serde(rename = "subsonic-response")]
    subsonic: SongSubsonic,
}

#[derive(Debug, Deserialize)]
struct SongSubsonic {
    status: Option<String>,
    song: Option<SongMetadata>,
}

#[derive(Debug, Deserialize)]
struct SongMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<i64>,
}

async fn fetch_song_metadata(
    state: &AppState,
    original_query: &str,
    id: &str,
) -> anyhow::Result<Option<SongMetadata>> {
    let query = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in url::form_urlencoded::parse(original_query.as_bytes()) {
            if key != "id" && key != "f" {
                serializer.append_pair(&key, &value);
            }
        }
        serializer.append_pair("id", id);
        serializer.append_pair("f", "json");
        serializer.finish()
    };
    let req = Request::builder()
        .uri(format!("/rest/getSong?{query}"))
        .body(Body::empty())?;
    let (status, _headers, body) = passthrough::fetch_upstream_identity(state, req).await?;
    if !status.is_success() {
        return Ok(None);
    }
    let envelope: SongEnvelope = serde_json::from_slice(&body)?;
    if envelope.subsonic.status.as_deref() != Some("ok") {
        return Ok(None);
    }
    Ok(envelope.subsonic.song)
}

fn has_lyrics(body: &str, format: Format) -> bool {
    match format {
        Format::Json => serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| {
                let lyrics = &value["subsonic-response"]["lyricsList"]["structuredLyrics"];
                if lyrics.is_array() {
                    Some(!lyrics.as_array().unwrap().is_empty())
                } else if lyrics.is_object() {
                    Some(true)
                } else {
                    Some(false)
                }
            })
            .unwrap_or(false),
        Format::Xml => {
            let mut reader = quick_xml::Reader::from_str(body);
            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e))
                        if e.local_name().as_ref() == b"structuredLyrics" =>
                    {
                        return true;
                    }
                    Ok(Event::Eof) => return false,
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
        }
    }
}

fn is_subsonic_response(body: &str, format: Format) -> bool {
    match format {
        Format::Json => serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| value["subsonic-response"]["status"].as_str().map(|_| true))
            .unwrap_or(false),
        Format::Xml => {
            let mut reader = quick_xml::Reader::from_str(body);
            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) | Ok(Event::Empty(e))
                        if e.local_name().as_ref() == b"subsonic-response" =>
                    {
                        return e
                            .attributes()
                            .flatten()
                            .any(|attr| attr.key.as_ref() == b"status");
                    }
                    Ok(Event::Eof) => return false,
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
        }
    }
}

fn lyrics_payload(lyrics: Option<Lyrics>) -> Payload<'static> {
    let Some(lyrics) = lyrics else {
        return empty_payload();
    };
    let synced_lines = lyrics.synced.clone().unwrap_or_default();
    let plain_lines = plain_lines(lyrics.plain.as_deref());
    if synced_lines.is_empty() && plain_lines.is_empty() {
        return empty_payload();
    }
    let json = structured_lyrics_json(&lyrics, &synced_lines, &plain_lines);
    Payload {
        key: "lyricsList",
        json,
        write_xml: Box::new(move |writer| {
            writer
                .write_event(Event::Start(BytesStart::new("lyricsList")))
                .unwrap();
            write_structured_xml(writer, &lyrics, &synced_lines, &plain_lines);
            writer
                .write_event(Event::End(BytesEnd::new("lyricsList")))
                .unwrap();
        }),
    }
}

fn empty_payload() -> Payload<'static> {
    Payload {
        key: "lyricsList",
        json: serde_json::json!({"structuredLyrics": []}),
        write_xml: Box::new(|writer| {
            writer
                .write_event(Event::Start(BytesStart::new("lyricsList")))
                .unwrap();
            writer
                .write_event(Event::End(BytesEnd::new("lyricsList")))
                .unwrap();
        }),
    }
}

fn structured_lyrics_json(
    lyrics: &Lyrics,
    synced_lines: &[SyncedLine],
    plain_lines: &[String],
) -> serde_json::Value {
    let synced = !synced_lines.is_empty();
    let lines: Vec<serde_json::Value> = if synced {
        synced_lines
            .iter()
            .map(|line| serde_json::json!({"start": line.start_ms, "value": line.value}))
            .collect()
    } else {
        plain_lines
            .iter()
            .map(|line| serde_json::json!({"value": line}))
            .collect()
    };
    serde_json::json!({
        "structuredLyrics": [{
            "displayArtist": lyrics.artist,
            "displayTitle": lyrics.title,
            "lang": "und",
            "synced": synced,
            "line": lines,
        }]
    })
}

fn write_structured_xml(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    lyrics: &Lyrics,
    synced_lines: &[SyncedLine],
    plain_lines: &[String],
) {
    let synced = !synced_lines.is_empty();
    let mut structured = BytesStart::new("structuredLyrics");
    structured.push_attribute(("displayArtist", lyrics.artist.as_str()));
    structured.push_attribute(("displayTitle", lyrics.title.as_str()));
    structured.push_attribute(("lang", "und"));
    structured.push_attribute(("synced", if synced { "true" } else { "false" }));
    writer.write_event(Event::Start(structured)).unwrap();

    if synced {
        for line in synced_lines {
            let mut node = BytesStart::new("line");
            let start = line.start_ms.to_string();
            node.push_attribute(("start", start.as_str()));
            node.push_attribute(("value", line.value.as_str()));
            writer.write_event(Event::Empty(node)).unwrap();
        }
    } else {
        for line in plain_lines {
            let mut node = BytesStart::new("line");
            node.push_attribute(("value", line.as_str()));
            writer.write_event(Event::Empty(node)).unwrap();
        }
    }

    writer
        .write_event(Event::End(BytesEnd::new("structuredLyrics")))
        .unwrap();
}

fn plain_lines(plain: Option<&str>) -> Vec<String> {
    plain
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lyrics() -> Lyrics {
        Lyrics {
            artist: "Янка".into(),
            title: "Про чертиков".into(),
            plain: Some("Первая\n\nВторая".into()),
            synced: None,
        }
    }

    #[test]
    fn structured_plain_json_uses_open_subsonic_shape() {
        let lyrics = sample_lyrics();
        let json = structured_lyrics_json(&lyrics, &[], &plain_lines(lyrics.plain.as_deref()));
        let entry = &json["structuredLyrics"][0];
        assert_eq!(entry["displayArtist"], "Янка");
        assert_eq!(entry["displayTitle"], "Про чертиков");
        assert_eq!(entry["synced"], false);
        assert_eq!(entry["line"].as_array().unwrap().len(), 2);
        assert_eq!(entry["line"][0]["value"], "Первая");
    }

    #[test]
    fn detects_singleton_or_array_lyrics_json() {
        assert!(!has_lyrics(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[]}}}"#,
            Format::Json
        ));
        assert!(has_lyrics(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":{"synced":false}}}}"#,
            Format::Json
        ));
        assert!(has_lyrics(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[{"line":[]}]}}}"#,
            Format::Json
        ));
    }

    #[test]
    fn detects_lyrics_xml() {
        assert!(!has_lyrics(
            r#"<subsonic-response status="ok"><lyricsList/></subsonic-response>"#,
            Format::Xml
        ));
        assert!(has_lyrics(
            r#"<subsonic-response status="ok"><lyricsList><structuredLyrics synced="false"/></lyricsList></subsonic-response>"#,
            Format::Xml
        ));
    }

    #[test]
    fn failed_subsonic_response_can_still_be_a_lyrics_fallback_candidate() {
        assert!(is_subsonic_response(
            r#"{"subsonic-response":{"status":"failed","error":{"code":70,"message":"data not found"}}}"#,
            Format::Json
        ));
        assert!(is_subsonic_response(
            r#"<subsonic-response status="failed"><error code="70" message="data not found"/></subsonic-response>"#,
            Format::Xml
        ));
    }
}
