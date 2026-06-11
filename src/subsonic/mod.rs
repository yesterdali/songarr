pub mod auth;
pub mod types;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// Response format requested by the client via `f=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Xml,
}

impl Format {
    /// `jsonp` and anything unknown map to None — those requests are left to
    /// plain passthrough rather than risking a malformed re-serialization.
    pub fn from_query_value(value: Option<&str>) -> Option<Self> {
        match value {
            None | Some("xml") => Some(Format::Xml),
            Some("json") => Some(Format::Json),
            _ => None,
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Format::Json => "application/json",
            Format::Xml => "application/xml",
        }
    }
}

/// Envelope attributes mirrored from Navidrome so synthesized responses are
/// indistinguishable from proxied ones.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub version: String,
    pub server_type: String,
    pub server_version: String,
    pub open_subsonic: bool,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            version: "1.16.1".into(),
            server_type: "navidrome".into(),
            server_version: String::new(),
            open_subsonic: true,
        }
    }
}

impl Envelope {
    /// Wrap a payload (e.g. `"song": {…}`) in a full subsonic-response, in
    /// the requested format. `payload` is (key, json value, xml writer fn).
    pub fn render_ok(&self, format: Format, payload: Option<types::Payload<'_>>) -> Response {
        self.render(format, "ok", None, payload)
    }

    pub fn render_error(&self, format: Format, code: u32, message: &str) -> Response {
        self.render(format, "failed", Some((code, message)), None)
    }

    fn render(
        &self,
        format: Format,
        status: &str,
        error: Option<(u32, &str)>,
        payload: Option<types::Payload<'_>>,
    ) -> Response {
        let body = match format {
            Format::Json => {
                let mut envelope = serde_json::json!({
                    "status": status,
                    "version": self.version,
                    "type": self.server_type,
                    "serverVersion": self.server_version,
                    "openSubsonic": self.open_subsonic,
                });
                if let Some((code, message)) = error {
                    envelope["error"] = serde_json::json!({"code": code, "message": message});
                }
                if let Some(payload) = &payload {
                    envelope[payload.key] = payload.json.clone();
                }
                serde_json::json!({ "subsonic-response": envelope }).to_string()
            }
            Format::Xml => {
                use quick_xml::events::{BytesDecl, BytesStart, Event};
                let mut writer = quick_xml::Writer::new(Vec::new());
                writer
                    .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
                    .unwrap();
                let mut root = BytesStart::new("subsonic-response");
                root.push_attribute(("xmlns", "http://subsonic.org/restapi"));
                root.push_attribute(("status", status));
                root.push_attribute(("version", self.version.as_str()));
                root.push_attribute(("type", self.server_type.as_str()));
                root.push_attribute(("serverVersion", self.server_version.as_str()));
                root.push_attribute((
                    "openSubsonic",
                    if self.open_subsonic { "true" } else { "false" },
                ));
                writer.write_event(Event::Start(root)).unwrap();
                if let Some((code, message)) = error {
                    let mut e = BytesStart::new("error");
                    e.push_attribute(("code", code.to_string().as_str()));
                    e.push_attribute(("message", message));
                    writer.write_event(Event::Empty(e)).unwrap();
                }
                if let Some(payload) = &payload {
                    (payload.write_xml)(&mut writer);
                }
                writer
                    .write_event(Event::End(quick_xml::events::BytesEnd::new(
                        "subsonic-response",
                    )))
                    .unwrap();
                String::from_utf8(writer.into_inner()).unwrap()
            }
        };
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, format.content_type())],
            body,
        )
            .into_response()
    }
}

/// Subsonic error 70: requested data not found.
pub fn error_not_found(envelope: &Envelope, format: Format) -> Response {
    envelope.render_error(format, 70, "The requested data was not found")
}
