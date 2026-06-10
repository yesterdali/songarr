//! getLyricsBySongId tolerance for virtual ids: empty success, never a 500
//! (protocol rule #6). Real ids pass through.

use axum::extract::{Request, State};
use axum::response::Response;
use quick_xml::events::{BytesEnd, BytesStart, Event};

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

    if !vtrack::is_virtual_id(&id) {
        return passthrough::handler(State(state), req).await;
    }

    let payload = Payload {
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
    };
    state.envelope().await.render_ok(format, Some(payload))
}
