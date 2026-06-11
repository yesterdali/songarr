//! Shared helpers for integration tests. The proxy under test runs
//! in-process against the harness Navidrome (tests/harness/up.sh).

// Each integration test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use axum::body::Bytes;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use songarr_proxy::{build_app, config::Config, db, AppState};

pub const USER: &str = "admin";
pub const PASSWORD: &str = "songarr-test";

/// Tests mutate shared Navidrome state (scrobbles, scans), and each test
/// fetches direct + via-proxy expecting identical bytes — serialize them.
pub fn serial() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn navidrome_url() -> String {
    std::env::var("SONGARR_TEST_NAVIDROME").unwrap_or_else(|_| "http://127.0.0.1:14533".into())
}

/// Subsonic auth query string; `format` None = server default (XML).
pub fn auth_query(format: Option<&str>) -> String {
    let salt = "sgrtest";
    let token = format!("{:x}", md5::compute(format!("{PASSWORD}{salt}")));
    let mut query = format!("u={USER}&t={token}&s={salt}&v=1.16.1&c=songarr-test");
    if let Some(f) = format {
        query.push_str("&f=");
        query.push_str(f);
    }
    query
}

/// Boot the proxy in-process on an ephemeral port; returns its base URL.
/// External search is OFF: the passthrough suite must compare pure proxying.
pub async fn spawn_proxy(upstream: &str) -> String {
    spawn_proxy_with(upstream, |config| {
        config.external_search.enabled = false;
    })
    .await
}

/// Like `spawn_proxy`, with a config hook (e.g. point catalog at a mock).
pub async fn spawn_proxy_with(upstream: &str, customize: impl FnOnce(&mut Config)) -> String {
    spawn_proxy_handle(upstream, customize).await.url
}

/// A spawned in-process proxy plus the paths tests may want to inspect.
pub struct TestProxy {
    pub url: String,
    pub db_path: std::path::PathBuf,
    pub staging_dir: std::path::PathBuf,
}

impl TestProxy {
    /// Open the proxy's SQLite db for direct assertions.
    pub async fn db(&self) -> sqlx::SqlitePool {
        db::init(&self.db_path).await.expect("open test db")
    }
}

pub async fn spawn_proxy_handle(upstream: &str, customize: impl FnOnce(&mut Config)) -> TestProxy {
    spawn_proxy_inner(upstream, customize, false).await
}

/// Proxy plus a running ingest worker (M4 tests).
pub async fn spawn_proxy_with_worker(
    upstream: &str,
    customize: impl FnOnce(&mut Config),
) -> TestProxy {
    spawn_proxy_inner(upstream, customize, true).await
}

async fn spawn_proxy_inner(
    upstream: &str,
    customize: impl FnOnce(&mut Config),
    with_worker: bool,
) -> TestProxy {
    let scratch = std::env::temp_dir().join(format!("songarr-it-{}", uuid::Uuid::new_v4()));
    let mut config = Config::default();
    // Never let tests (e.g. prefetch-on-search) reach a real yt-dlp, and
    // keep the innertube fast path opt-in (tests point it at a mock).
    config.streaming.ytdlp_path =
        format!("{}/tests/harness/bin/yt-dlp", env!("CARGO_MANIFEST_DIR"));
    config.streaming.innertube = false;
    config.navidrome.base_url = upstream.to_string();
    config.navidrome.admin_user = USER.into();
    config.navidrome.admin_password = PASSWORD.into();
    config.server.db_path = scratch.join("songarr.db");
    config.library.staging_dir = scratch.join("staging");
    customize(&mut config);
    let db_path = config.server.db_path.clone();
    let staging_dir = config.library.staging_dir.clone();

    let pool = db::init(&config.server.db_path).await.expect("db init");
    let state = AppState::new(std::sync::Arc::new(config), pool).expect("app state");
    if with_worker {
        tokio::spawn(songarr_proxy::ingest::worker(state.clone()));
    }
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    TestProxy {
        url: format!("http://{addr}"),
        db_path,
        staging_dir,
    }
}

pub async fn fetch(base: &str, path_and_query: &str) -> (StatusCode, HeaderMap, Bytes) {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client
        .get(format!("{base}{path_and_query}"))
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {path_and_query} failed: {e}"));
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("reading body");
    (status, headers, body)
}

pub async fn fetch_json(base: &str, path_and_query: &str) -> serde_json::Value {
    let (status, _, body) = fetch(base, path_and_query).await;
    assert_eq!(status, StatusCode::OK, "GET {path_and_query} -> {status}");
    serde_json::from_slice(&body)
        .unwrap_or_else(|e| panic!("GET {path_and_query} returned invalid JSON: {e}"))
}

/// Fake JPEG payload (magic bytes + filler) served by the mock Deezer.
pub const FAKE_JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0xD9,
];

/// Mock api.deezer.com: /search returns two fixed tracks (one colliding
/// with the seeded library for dedup tests), /art/{id} serves fake artwork
/// and counts hits. Returns (base_url, artwork_hit_counter).
pub async fn spawn_mock_deezer() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use axum::extract::{Path, Query};
    use axum::routing::get;
    use axum::{Json, Router};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let art_hits = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let search_base = base.clone();
    let hits = art_hits.clone();
    let router = Router::new()
        .route(
            "/search",
            get(move |Query(params): Query<HashMap<String, String>>| {
                let base = search_base.clone();
                async move {
                    let q = params.get("q").cloned().unwrap_or_default();
                    let mut data = if q.contains("Mock") || q.contains("Tone") {
                        serde_json::json!([
                            {
                                "id": 901,
                                "title": "Mock Song One",
                                "duration": 180,
                                "artist": {"name": "Mock Artist"},
                                "album": {
                                    "title": "Mock Album",
                                    "cover_big": format!("{base}/art/901.jpg"),
                                    "cover_xl": format!("{base}/art/901.jpg")
                                },
                                "type": "track"
                            },
                            {
                                // Collides with seeded "The Sine Waves — Tone 220 Hz" (4s).
                                "id": 902,
                                "title": "Tone 220 Hz",
                                "duration": 4,
                                "artist": {"name": "The Sine Waves"},
                                "album": {
                                    "title": "Pure Tones",
                                    "cover_big": format!("{base}/art/902.jpg")
                                },
                                "type": "track"
                            }
                        ])
                    } else {
                        serde_json::json!([])
                    };
                    // "Low" requests add a track whose duration disagrees
                    // wildly with the mock yt-dlp result → low match score.
                    if q.contains("Low") {
                        data.as_array_mut().unwrap().push(serde_json::json!({
                            "id": 903,
                            "title": "Mock Song Low",
                            "duration": 999,
                            "artist": {"name": "Mock Artist"},
                            "album": {"title": "Mock Album"},
                            "type": "track"
                        }));
                    }
                    Json(serde_json::json!({"data": data, "total": 3}))
                }
            }),
        )
        .route(
            "/art/{file}",
            get(move |Path(_file): Path<String>| {
                let hits = hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    ([("content-type", "image/jpeg")], FAKE_JPEG.to_vec())
                }
            }),
        );

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (base, art_hits)
}

/// Mock YouTube: innertube `player` endpoint returning a direct audio URL
/// served by the same mock (the seed fixture webm). Returns the base URL.
pub async fn spawn_mock_youtube() -> String {
    use axum::routing::{get, post};
    use axum::{Json, Router};

    fn fixture() -> String {
        format!(
            "{}/tests/harness/data/fixtures/source.webm",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let media_base = base.clone();
    let router = Router::new()
        .route(
            "/youtubei/v1/player",
            post(move || {
                let base = media_base.clone();
                async move {
                    let size = std::fs::metadata(fixture()).map(|m| m.len()).unwrap_or(0);
                    Json(serde_json::json!({
                        "playabilityStatus": {"status": "OK"},
                        "streamingData": {
                            "adaptiveFormats": [
                                {
                                    "itag": 251,
                                    "mimeType": "audio/webm; codecs=\"opus\"",
                                    "bitrate": 160000,
                                    "contentLength": size.to_string(),
                                    "url": format!("{base}/media/audio.webm")
                                },
                                {
                                    "itag": 137,
                                    "mimeType": "video/mp4; codecs=\"avc1\"",
                                    "bitrate": 2000000,
                                    "url": format!("{base}/media/video.mp4")
                                }
                            ]
                        }
                    }))
                }
            }),
        )
        .route(
            "/media/audio.webm",
            // Mirror measured googlevideo behavior (iOS-client URLs): the
            // range must come as a `range=start-end` QUERY PARAM (no param →
            // 403, an HTTP Range header alone → 403) and a span over ~1 MiB
            // 403s too. Models a non-token-gated server — the case where the
            // innertube path is worth enabling at all.
            get(
                |axum::extract::Query(q): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| async move {
                    use axum::http::StatusCode;
                    let parsed = q
                        .get("range")
                        .and_then(|r| r.split_once('-'))
                        .and_then(|(s, e)| Some((s.parse::<u64>().ok()?, e.parse::<u64>().ok()?)));
                    let span_ok =
                        parsed.is_some_and(|(s, e)| e >= s && e - s < 1024 * 1024 + 48432);
                    let Some((start, end)) = parsed.filter(|_| span_ok) else {
                        return (
                            StatusCode::FORBIDDEN,
                            [("content-type", "text/plain")],
                            Vec::new(),
                        );
                    };
                    let bytes = std::fs::read(fixture()).expect("fixture missing — run seed.sh");
                    let start = (start.min(bytes.len() as u64)) as usize;
                    let end = ((end + 1).min(bytes.len() as u64)) as usize;
                    (
                        StatusCode::OK,
                        [("content-type", "audio/webm")],
                        bytes[start..end].to_vec(),
                    )
                },
            ),
        );

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    base
}

/// Navidrome serializes artist `roles` in nondeterministic order (Go map
/// iteration), so two identical requests can differ byte-wise upstream.
/// Canonicalize that before comparing direct vs proxied bodies; binary
/// bodies (stream, cover art) pass through untouched.
pub fn normalize_body(body: &[u8]) -> Vec<u8> {
    if body.first() == Some(&b'{') {
        if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(body) {
            sort_string_arrays(&mut value);
            return serde_json::to_vec(&value).unwrap();
        }
    }
    if let Ok(text) = std::str::from_utf8(body) {
        if text.contains("<roles>") {
            return sort_xml_roles(text).into_bytes();
        }
    }
    body.to_vec()
}

fn sort_string_arrays(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                sort_string_arrays(item);
            }
            if items.iter().all(serde_json::Value::is_string) {
                items.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            }
        }
        serde_json::Value::Object(map) => {
            for (_, item) in map.iter_mut() {
                sort_string_arrays(item);
            }
        }
        _ => {}
    }
}

/// Sort each consecutive run of `<roles>…</roles>` elements.
fn sort_xml_roles(body: &str) -> String {
    const OPEN: &str = "<roles>";
    const CLOSE: &str = "</roles>";
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let mut cursor = &rest[start..];
        let mut roles = Vec::new();
        while cursor.starts_with(OPEN) {
            let Some(end) = cursor.find(CLOSE) else { break };
            roles.push(&cursor[OPEN.len()..end]);
            cursor = &cursor[end + CLOSE.len()..];
        }
        roles.sort_unstable();
        for role in roles {
            out.push_str(OPEN);
            out.push_str(role);
            out.push_str(CLOSE);
        }
        rest = cursor;
    }
    out.push_str(rest);
    out
}

#[test]
fn normalize_sorts_json_string_arrays_only() {
    let a = br#"{"x":{"roles":["b","a"],"songs":[{"id":"2"},{"id":"1"}]}}"#;
    let b = br#"{"x":{"roles":["a","b"],"songs":[{"id":"2"},{"id":"1"}]}}"#;
    assert_eq!(normalize_body(a), normalize_body(b));
    // Object array order is preserved (a real difference must still fail).
    let c = br#"{"x":{"roles":["a","b"],"songs":[{"id":"1"},{"id":"2"}]}}"#;
    assert_ne!(normalize_body(b), normalize_body(c));
}

#[test]
fn normalize_sorts_xml_role_runs() {
    let a = b"<artist x=\"1\"><roles>b</roles><roles>a</roles></artist>";
    let b = b"<artist x=\"1\"><roles>a</roles><roles>b</roles></artist>";
    assert_eq!(normalize_body(a), normalize_body(b));
}

#[test]
fn normalize_leaves_binary_untouched() {
    let raw = [0xffu8, 0xfb, 0x90, 0x00, 0x7b];
    assert_eq!(normalize_body(&raw), raw.to_vec());
}

/// Block until the harness library is scanned and visible; triggers a scan
/// on first need. Panics with a hint if the harness isn't running.
pub async fn ensure_library_scanned(navidrome: &str) {
    let auth = auth_query(Some("json"));
    for attempt in 0..120 {
        let body = fetch_json(navidrome, &format!("/rest/getArtists?{auth}")).await;
        let indexes = body["subsonic-response"]["artists"]["index"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        if indexes > 0 {
            return;
        }
        if attempt == 0 {
            fetch(navidrome, &format!("/rest/startScan?{auth}")).await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("navidrome library is empty after 60s — did tests/harness/up.sh run?");
}
