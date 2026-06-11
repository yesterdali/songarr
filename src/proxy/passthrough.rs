//! Transparent reverse proxy to Navidrome (M1).
//!
//! Everything not handled by an explicit route lands here: method, raw
//! path+query, headers and bodies are forwarded verbatim in both directions,
//! streaming. The body is never inspected or re-encoded, so compressed
//! responses, range requests and SSE (`/api/events`) pass through unchanged.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{
    HeaderMap, HeaderName, CONNECTION, CONTENT_LENGTH, HOST, PROXY_AUTHENTICATE,
    PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// Request bodies with a Content-Length are buffered (they are tiny Subsonic
/// form posts); this caps abuse. Chunked request bodies are streamed.
const MAX_BUFFERED_REQUEST_BODY: usize = 16 * 1024 * 1024;

pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let path = req.uri().path().to_owned();
    match forward(&state, req).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, %path, "passthrough to navidrome failed");
            (StatusCode::BAD_GATEWAY, "upstream unavailable").into_response()
        }
    }
}

pub async fn forward(state: &AppState, req: Request) -> anyhow::Result<Response> {
    let upstream = send_upstream(state, req, false).await?;

    let status = upstream.status();
    let mut headers = upstream.headers().clone();
    strip_hop_by_hop(&mut headers);

    let mut response = Response::builder()
        .status(status)
        .body(Body::from_stream(upstream.bytes_stream()))?;
    *response.headers_mut() = headers;
    Ok(response)
}

/// Forward a request upstream with `Accept-Encoding: identity` and buffer
/// the whole response — for endpoints whose body the proxy will modify
/// (protocol rule: never edit a compressed body).
pub async fn fetch_upstream_identity(
    state: &AppState,
    req: Request,
) -> anyhow::Result<(StatusCode, HeaderMap, axum::body::Bytes)> {
    let upstream = send_upstream(state, req, true).await?;
    let status = upstream.status();
    let mut headers = upstream.headers().clone();
    strip_hop_by_hop(&mut headers);
    let body = upstream.bytes().await?;
    Ok((status, headers, body))
}

async fn send_upstream(
    state: &AppState,
    req: Request,
    identity_encoding: bool,
) -> anyhow::Result<reqwest::Response> {
    let (parts, body) = req.into_parts();

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let url = build_upstream_url(&state.config.navidrome.base_url, path_and_query);

    let mut headers = parts.headers;
    strip_hop_by_hop(&mut headers);
    // reqwest derives Host from the URL; a stale client Host would confuse
    // upstream vhosting. Content-Length is re-derived from the body below.
    headers.remove(HOST);
    if identity_encoding {
        headers.insert(
            axum::http::header::ACCEPT_ENCODING,
            axum::http::HeaderValue::from_static("identity"),
        );
    }
    let content_length = headers.remove(CONTENT_LENGTH);

    let mut upstream_req = state
        .http
        .request(parts.method.clone(), url)
        .headers(headers);

    upstream_req = if let Some(len) = &content_length {
        let limit = len
            .to_str()
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&v| v <= MAX_BUFFERED_REQUEST_BODY)
            .ok_or_else(|| anyhow::anyhow!("unacceptable request content-length {len:?}"))?;
        let bytes = axum::body::to_bytes(body, limit).await?;
        upstream_req.body(bytes)
    } else if parts.method == axum::http::Method::GET || parts.method == axum::http::Method::HEAD {
        upstream_req
    } else {
        // No Content-Length on a method that may carry a body: stream it
        // through chunked.
        upstream_req.body(reqwest::Body::wrap_stream(body.into_data_stream()))
    };

    Ok(upstream_req.send().await?)
}

fn build_upstream_url(base_url: &str, path_and_query: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path_and_query)
}

/// Remove hop-by-hop headers (RFC 9110 §7.6.1) plus anything named in
/// `Connection`. Everything else — including Accept-Encoding, Content-Type,
/// ETag, Range — is forwarded untouched.
fn strip_hop_by_hop(headers: &mut HeaderMap) {
    let connection_named: Vec<HeaderName> = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect();
    for name in connection_named {
        headers.remove(name);
    }
    for name in [
        CONNECTION,
        PROXY_AUTHENTICATE,
        PROXY_AUTHORIZATION,
        TE,
        TRAILER,
        TRANSFER_ENCODING,
        UPGRADE,
    ] {
        headers.remove(name);
    }
    headers.remove(HeaderName::from_static("keep-alive"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn upstream_url_preserves_encoded_query() {
        let url = build_upstream_url(
            "http://navidrome:4533/",
            "/rest/search3?query=a%20b%26c&f=json",
        );
        assert_eq!(
            url,
            "http://navidrome:4533/rest/search3?query=a%20b%26c&f=json"
        );
    }

    #[test]
    fn upstream_url_without_query() {
        assert_eq!(
            build_upstream_url("http://navidrome:4533", "/app/"),
            "http://navidrome:4533/app/"
        );
    }

    #[test]
    fn hop_by_hop_headers_are_stripped() {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("close, x-custom-hop"));
        headers.insert("x-custom-hop", HeaderValue::from_static("1"));
        headers.insert(TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
        headers.insert("keep-alive", HeaderValue::from_static("timeout=5"));
        headers.insert("accept-encoding", HeaderValue::from_static("gzip"));
        headers.insert("range", HeaderValue::from_static("bytes=0-100"));

        strip_hop_by_hop(&mut headers);

        assert!(headers.get(CONNECTION).is_none());
        assert!(headers.get("x-custom-hop").is_none());
        assert!(headers.get(TRANSFER_ENCODING).is_none());
        assert!(headers.get("keep-alive").is_none());
        // End-to-end headers survive.
        assert_eq!(headers.get("accept-encoding").unwrap(), "gzip");
        assert_eq!(headers.get("range").unwrap(), "bytes=0-100");
    }
}
