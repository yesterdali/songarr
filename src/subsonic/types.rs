//! Subsonic entity synthesis for virtual tracks.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Writer;

use crate::config::{StreamFormat, Streaming};
use crate::vtrack::VirtualTrack;

pub type XmlWriteFn<'a> = Box<dyn Fn(&mut Writer<Vec<u8>>) + Send + 'a>;

/// A payload to embed in a subsonic-response: JSON value plus an XML writer.
pub struct Payload<'a> {
    pub key: &'a str,
    pub json: serde_json::Value,
    pub write_xml: XmlWriteFn<'a>,
}

/// A synthesized `song` (Subsonic `Child`) for a virtual track.
#[derive(Debug, Clone)]
pub struct SongEntry {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub cover_art: Option<String>,
    pub duration_secs: Option<i64>,
    pub suffix: &'static str,
    pub content_type: &'static str,
    pub bit_rate: u32,
    pub size: Option<i64>,
    pub created: String,
}

impl SongEntry {
    pub fn from_virtual(track: &VirtualTrack, streaming: &Streaming) -> Self {
        let (suffix, content_type) = match streaming.format {
            StreamFormat::Opus => ("opus", "audio/ogg"),
            StreamFormat::Mp3 => ("mp3", "audio/mpeg"),
        };
        let duration_secs = track.duration_ms.map(|ms| ms / 1000);
        // Some clients require size/bitRate to display or play an entry;
        // estimate from the live-pipe bitrate (kbps → bytes/s is ×125).
        let size = duration_secs.map(|d| d * i64::from(streaming.bitrate_kbps) * 125);
        Self {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            cover_art: track.artwork_url.as_ref().map(|_| track.id.clone()),
            duration_secs,
            suffix,
            content_type,
            bit_rate: streaming.bitrate_kbps,
            size,
            created: crate::vtrack::now_utc(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut song = serde_json::json!({
            "id": self.id,
            "isDir": false,
            "isVideo": false,
            "type": "music",
            "title": self.title,
            "artist": self.artist,
            "suffix": self.suffix,
            "contentType": self.content_type,
            "bitRate": self.bit_rate,
            "created": self.created,
        });
        if let Some(album) = &self.album {
            song["album"] = serde_json::json!(album);
        }
        if let Some(cover) = &self.cover_art {
            song["coverArt"] = serde_json::json!(cover);
        }
        if let Some(duration) = self.duration_secs {
            song["duration"] = serde_json::json!(duration);
        }
        if let Some(size) = self.size {
            song["size"] = serde_json::json!(size);
        }
        song
    }

    /// Write as `<song …/>` (or another element name, e.g. for search2).
    pub fn write_xml(&self, writer: &mut Writer<Vec<u8>>, element: &str) {
        let mut song = BytesStart::new(element);
        song.push_attribute(("id", self.id.as_str()));
        song.push_attribute(("isDir", "false"));
        song.push_attribute(("isVideo", "false"));
        song.push_attribute(("type", "music"));
        song.push_attribute(("title", self.title.as_str()));
        song.push_attribute(("artist", self.artist.as_str()));
        if let Some(album) = &self.album {
            song.push_attribute(("album", album.as_str()));
        }
        if let Some(cover) = &self.cover_art {
            song.push_attribute(("coverArt", cover.as_str()));
        }
        if let Some(duration) = self.duration_secs {
            song.push_attribute(("duration", duration.to_string().as_str()));
        }
        song.push_attribute(("suffix", self.suffix));
        song.push_attribute(("contentType", self.content_type));
        song.push_attribute(("bitRate", self.bit_rate.to_string().as_str()));
        if let Some(size) = self.size {
            song.push_attribute(("size", size.to_string().as_str()));
        }
        song.push_attribute(("created", self.created.as_str()));
        writer.write_event(Event::Empty(song)).unwrap();
    }

    /// Payload for a synthesized `getSong` response.
    pub fn into_payload(self) -> Payload<'static> {
        let json = self.to_json();
        Payload {
            key: "song",
            json,
            write_xml: Box::new(move |writer| self.write_xml(writer, "song")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track() -> VirtualTrack {
        VirtualTrack {
            id: "sgr_test123".into(),
            provider: "deezer".into(),
            provider_track_id: "1".into(),
            artist: "Ärtist \"Quoted\" & Co".into(),
            title: "Tïtle <One>".into(),
            album: Some("Älbum".into()),
            duration_ms: Some(224_000),
            isrc: None,
            artwork_url: Some("https://example.com/a.jpg".into()),
            status: "virtual".into(),
            real_subsonic_id: None,
            resolved_url: None,
            resolved_score: None,
            resolved_title: None,
            resolved_at_epoch: None,
        }
    }

    #[test]
    fn json_entry_has_required_fields() {
        let entry = SongEntry::from_virtual(&sample_track(), &Streaming::default());
        let json = entry.to_json();
        assert_eq!(json["id"], "sgr_test123");
        assert_eq!(json["duration"], 224);
        assert_eq!(json["suffix"], "opus");
        assert_eq!(json["contentType"], "audio/ogg");
        assert_eq!(json["coverArt"], "sgr_test123");
        assert_eq!(json["bitRate"], 160);
        assert_eq!(json["size"], 224 * 160 * 125);
        assert_eq!(json["isDir"], false);
    }

    #[test]
    fn xml_entry_escapes_special_chars() {
        let entry = SongEntry::from_virtual(&sample_track(), &Streaming::default());
        let mut writer = Writer::new(Vec::new());
        entry.write_xml(&mut writer, "song");
        let xml = String::from_utf8(writer.into_inner()).unwrap();
        assert!(xml.starts_with("<song "), "{xml}");
        assert!(xml.contains("&quot;Quoted&quot;"), "{xml}");
        assert!(xml.contains("&lt;One&gt;"), "{xml}");
        assert!(xml.contains("duration=\"224\""), "{xml}");
        // Valid XML: re-parse cleanly.
        let mut reader = quick_xml::Reader::from_str(&xml);
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("invalid xml: {e}\n{xml}"),
            }
        }
    }
}
