//! M3 integration suite: stream-on-demand with mock yt-dlp (offline,
//! deterministic) against the harness Navidrome.
//!
//! Requires `tests/harness/up.sh`, then
//! `cargo test --test virtual_stream -- --ignored`.

#![allow(clippy::await_holding_lock)]

mod common;

use std::path::PathBuf;
use std::time::Duration;

use axum::body::Bytes;
use axum::routing::get;
use axum::Router;
use common::*;
use reqwest::StatusCode;
use songarr_proxy::config::Config;
use songarr_proxy::vtrack::{self, CatalogTrack};

fn mock_ytdlp(name: &str) -> String {
    format!("{}/tests/harness/bin/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/harness/data/fixtures/source.webm")
}

fn fixture_size() -> u64 {
    std::fs::metadata(fixture_path())
        .expect("fixture missing — run tests/harness/seed.sh")
        .len()
}

async fn spawn_mock_audio() -> String {
    let bytes = std::fs::read(fixture_path()).expect("fixture exists");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().route(
        "/audio.webm",
        get(move || {
            let bytes = bytes.clone();
            async move { ([("content-type", "audio/webm")], Bytes::from(bytes)) }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("{base}/audio.webm")
}

fn mock_yandex_helper(url: &str) -> String {
    let dir = std::env::temp_dir().join(format!("songarr-yandex-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("songarr-yandex");
    std::fs::write(
        &path,
        format!(
            r#"#!/usr/bin/env python3
import json, sys
cmd = sys.argv[1]
if cmd == "download":
    print(json.dumps({{"url": "{url}", "codec": "webm", "bitrateKbps": 128}}))
elif cmd in ("wave", "search"):
    print("[]")
else:
    sys.exit(2)
"#
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path.to_string_lossy().into_owned()
}

async fn spawn_m3_proxy(deezer: &str, customize: impl FnOnce(&mut Config)) -> TestProxy {
    let deezer = deezer.to_string();
    spawn_proxy_handle(&navidrome_url(), move |config| {
        config.external_search.api_base_deezer = deezer;
        config.streaming.ytdlp_path = mock_ytdlp("yt-dlp");
        customize(config);
    })
    .await
}

async fn insert_yandex_track(proxy: &TestProxy) -> String {
    let pool = proxy.db().await;
    vtrack::upsert(
        &pool,
        &CatalogTrack {
            provider: "yandex",
            provider_track_id: "ya-track-1".into(),
            artist: "Yandex Artist".into(),
            title: "Yandex Direct".into(),
            album: Some("Yandex Album".into()),
            duration_ms: Some(180_000),
            isrc: None,
            artwork_url: None,
        },
    )
    .await
    .unwrap()
}

/// Search through the proxy and return the virtual id of "Mock Song One".
async fn virtual_id(proxy_url: &str) -> String {
    let body = fetch_json(
        proxy_url,
        &format!(
            "/rest/search3?{}&query=Mock+Artist",
            auth_query(Some("json"))
        ),
    )
    .await;
    body["subsonic-response"]["searchResult3"]["song"]
        .as_array()
        .expect("song array")
        .iter()
        .find(|s| s["title"] == "Mock Song One")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .expect("virtual song injected")
}

async fn wait_for_job_status(proxy: &TestProxy, status: &str) -> (String, String) {
    let pool = proxy.db().await;
    for _ in 0..100 {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT id, COALESCE(staging_path, '') FROM stream_jobs WHERE status = ? LIMIT 1",
        )
        .bind(status)
        .fetch_optional(&pool)
        .await
        .unwrap();
        if let Some(row) = row {
            pool.close().await;
            return row;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let all: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, status, error FROM stream_jobs")
            .fetch_all(&pool)
            .await
            .unwrap();
    panic!("no stream_job reached status {status:?}; jobs: {all:?}");
}

fn assert_ffprobe_codec(path: &std::path::Path, expected: &str) {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("ffprobe runs");
    let codecs = String::from_utf8_lossy(&output.stdout);
    assert!(
        codecs.lines().any(|c| c.trim() == expected),
        "expected codec {expected} in {path:?}, got: {codecs}"
    );
}

#[tokio::test]
async fn yandex_virtual_stream_uses_helper_audio_first() {
    let _guard = serial();
    let source_url = spawn_mock_audio().await;
    let helper = mock_yandex_helper(&source_url);
    let proxy = spawn_proxy_handle("http://127.0.0.1:9", move |config| {
        config.yandex.enabled = true;
        config.yandex.access_token = "test-token".into();
        config.yandex.helper_path = helper;
        config.yandex.use_for_import = true;
    })
    .await;
    let id = insert_yandex_track(&proxy).await;

    let (status, headers, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "audio/ogg");
    assert!(body.starts_with(b"OggS"));

    let (_job, staging) = wait_for_job_status(&proxy, "finalizing").await;
    assert!(std::path::Path::new(&staging).exists());
    let pool = proxy.db().await;
    let row: (String, i64) =
        sqlx::query_as("SELECT source_url, match_score FROM stream_jobs LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, source_url);
    assert_eq!(row.1, 100);
}

#[tokio::test]
async fn yandex_virtual_stream_falls_back_to_youtube_when_helper_audio_fails() {
    let _guard = serial();
    assert!(
        fixture_path().exists(),
        "fixture missing — run tests/harness/seed.sh"
    );
    let helper = mock_yandex_helper("http://127.0.0.1:9/nope.webm");
    let proxy = spawn_proxy_handle("http://127.0.0.1:9", move |config| {
        config.streaming.ytdlp_path = mock_ytdlp("yt-dlp");
        config.yandex.enabled = true;
        config.yandex.access_token = "test-token".into();
        config.yandex.helper_path = helper;
        config.yandex.use_for_import = true;
    })
    .await;
    let id = insert_yandex_track(&proxy).await;

    let (status, headers, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "audio/ogg");
    assert!(body.starts_with(b"OggS"));

    let pool = proxy.db().await;
    let row: (String,) = sqlx::query_as("SELECT source_url FROM stream_jobs LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        row.0.starts_with("https://www.youtube.com/watch"),
        "expected YouTube fallback, got {}",
        row.0
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn virtual_stream_plays_and_stages() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |_| {}).await;
    let id = virtual_id(&proxy.url).await;

    let (status, headers, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "audio/ogg"
    );
    assert_eq!(headers.get("accept-ranges").unwrap(), "none");
    assert!(
        headers.get("content-length").is_none(),
        "length is unknowable"
    );
    assert!(body.starts_with(b"OggS"), "transcoded output is ogg/opus");
    assert!(body.len() > 100_000, "got {} bytes", body.len());

    // The original (pre-transcode) bytes must be fully staged.
    let (_job, staging_path) = wait_for_job_status(&proxy, "finalizing").await;
    let staged = std::fs::metadata(&staging_path).unwrap().len();
    assert_eq!(staged, fixture_size(), "staged file is the complete source");
    assert_ffprobe_codec(std::path::Path::new(&staging_path), "opus");

    // Track is marked staged, match score recorded.
    let pool = proxy.db().await;
    let (track_status, score): (String, i64) = sqlx::query_as(
        "SELECT vt.status, sj.match_score FROM virtual_tracks vt
         JOIN stream_jobs sj ON sj.virtual_track_id = vt.id WHERE vt.id = ?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(track_status, "staged");
    assert!(
        score > 80,
        "perfect mock match should score high, got {score}"
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn two_simultaneous_virtual_streams() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |_| {}).await;
    let id = virtual_id(&proxy.url).await;

    let auth = auth_query(None);
    let path = format!("/rest/stream?{auth}&id={id}");
    let (a, b) = tokio::join!(fetch(&proxy.url, &path), fetch(&proxy.url, &path));
    for (status, _, body) in [a, b] {
        assert_eq!(status, StatusCode::OK);
        assert!(body.starts_with(b"OggS"));
        assert!(body.len() > 100_000);
    }
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn disconnect_mid_play_still_completes_staging() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |_| {}).await;
    let id = virtual_id(&proxy.url).await;

    // Start playing, consume ~1s of the live pipe, then vanish.
    let client = reqwest::Client::new();
    let mut response = client
        .get(format!(
            "{}/rest/stream?{}&id={id}",
            proxy.url,
            auth_query(None)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while tokio::time::Instant::now() < deadline {
        if response.chunk().await.unwrap().is_none() {
            break;
        }
    }
    drop(response); // client disconnect

    // The acquisition must finish in the background.
    let (_job, staging_path) = wait_for_job_status(&proxy, "finalizing").await;
    for _ in 0..100 {
        if std::fs::metadata(&staging_path)
            .map(|m| m.len())
            .unwrap_or(0)
            == fixture_size()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        std::fs::metadata(&staging_path).unwrap().len(),
        fixture_size(),
        "download continued to completion after disconnect"
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn first_byte_timeout_returns_503() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |config| {
        config.streaming.ytdlp_path = mock_ytdlp("yt-dlp-slow");
        config.streaming.timeout_first_byte_secs = 2;
    })
    .await;
    let id = virtual_id(&proxy.url).await;

    let started = std::time::Instant::now();
    let (status, _, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}&f=json", auth_query(Some("json"))),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "timeout must trip near the configured 2s, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn scrobble_and_star_on_virtual_ids_are_queued() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |_| {}).await;
    let id = virtual_id(&proxy.url).await;

    // Both formats answer success without touching Navidrome.
    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/scrobble?{}&id={id}&submission=true",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");
    let (status, _, raw) = fetch(
        &proxy.url,
        &format!("/rest/star?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&raw).contains(r#"status="ok""#));

    let pool = proxy.db().await;
    let actions: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT action, username, payload_json FROM pending_actions
         WHERE virtual_track_id = ? ORDER BY action",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions.len(), 2, "{actions:?}");
    assert_eq!(actions[0].0, "scrobble");
    assert_eq!(actions[1].0, "star");
    assert_eq!(actions[0].1, USER);
    // Payload keeps the captured auth params for post-import replay.
    assert!(actions[0].2.contains("submission=true"), "{}", actions[0].2);
    assert!(actions[0].2.contains("u=admin"), "{}", actions[0].2);
    assert!(actions[0].2.contains("t="), "{}", actions[0].2);
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn lyrics_for_virtual_id_is_empty_success() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |_| {}).await;
    let id = virtual_id(&proxy.url).await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getLyricsBySongId?{}&id={id}",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");
    assert!(body["subsonic-response"]["lyricsList"]["structuredLyrics"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn search_prefetches_resolution_and_play_skips_search() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;

    let log_path = std::env::temp_dir().join(format!("ytdlp-pf-{}.log", uuid::Uuid::new_v4()));
    std::env::set_var("MOCK_YTDLP_LOG", &log_path);
    let proxy = spawn_m3_proxy(&deezer, |_| {}).await;
    let id = virtual_id(&proxy.url).await;

    // The search itself should trigger background resolution.
    let pool = proxy.db().await;
    let mut resolved: Option<(String, i64)> = None;
    for _ in 0..50 {
        let row: (Option<String>, Option<i64>) =
            sqlx::query_as("SELECT resolved_url, resolved_score FROM virtual_tracks WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        if let (Some(url), Some(score)) = row {
            resolved = Some((url, score));
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (url, score) = resolved.expect("prefetch must cache a resolution");
    assert!(url.contains("youtube.com/watch"), "{url}");
    assert!(score > 80, "{score}");

    let searches_after_prefetch = std::fs::read_to_string(&log_path)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains("ytsearch"))
        .count();

    // Play: must NOT run another yt-dlp search (cache hit), audio still fine.
    let (status, _, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    std::env::remove_var("MOCK_YTDLP_LOG");
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with(b"OggS"));

    let searches_after_play = std::fs::read_to_string(&log_path)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains("ytsearch"))
        .count();
    assert_eq!(
        searches_after_play, searches_after_prefetch,
        "cached play must not re-run the yt-dlp search"
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn innertube_direct_path_streams_without_ytdlp_download() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let youtube = spawn_mock_youtube().await;

    let log_path = std::env::temp_dir().join(format!("ytdlp-it-{}.log", uuid::Uuid::new_v4()));
    std::env::set_var("MOCK_YTDLP_LOG", &log_path);
    let proxy = spawn_m3_proxy(&deezer, |config| {
        config.streaming.innertube = true;
        config.streaming.innertube_api_base = youtube;
    })
    .await;
    let id = virtual_id(&proxy.url).await;

    let (status, headers, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    std::env::remove_var("MOCK_YTDLP_LOG");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "audio/ogg");
    assert!(body.starts_with(b"OggS"));

    // Original bytes staged via the direct HTTP source.
    let (_job, staging_path) = wait_for_job_status(&proxy, "finalizing").await;
    assert_eq!(
        std::fs::metadata(&staging_path).unwrap().len(),
        fixture_size()
    );

    // yt-dlp must only have been used for searches (prefetch), never for
    // the download — that's the whole point of the fast path.
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        log.lines().all(|l| l.contains("ytsearch")),
        "yt-dlp download invoked despite direct path:\n{log}"
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn innertube_failure_falls_back_to_ytdlp() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m3_proxy(&deezer, |config| {
        config.streaming.innertube = true;
        // Unreachable: the fast path must fail fast and degrade gracefully.
        config.streaming.innertube_api_base = "http://127.0.0.1:9".into();
    })
    .await;
    let id = virtual_id(&proxy.url).await;

    let (status, _, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "fallback must keep streaming working"
    );
    assert!(body.starts_with(b"OggS"));
    let (_job, staging_path) = wait_for_job_status(&proxy, "finalizing").await;
    assert_eq!(
        std::fs::metadata(&staging_path).unwrap().len(),
        fixture_size()
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn ytdlp_proxy_setting_is_passed_through() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;

    let log_path = std::env::temp_dir().join(format!("ytdlp-args-{}.log", uuid::Uuid::new_v4()));
    std::env::set_var("MOCK_YTDLP_LOG", &log_path);
    let proxy = spawn_m3_proxy(&deezer, |config| {
        config.streaming.ytdlp_proxy = "http://gluetun.test:8888".into();
    })
    .await;
    let id = virtual_id(&proxy.url).await;
    let (status, _, _) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    std::env::remove_var("MOCK_YTDLP_LOG");
    assert_eq!(status, StatusCode::OK);

    let log = std::fs::read_to_string(&log_path).unwrap();
    let proxied_lines = log
        .lines()
        .filter(|l| l.contains("--proxy http://gluetun.test:8888"))
        .count();
    assert!(
        proxied_lines >= 2,
        "both search and download must use --proxy; log:\n{log}"
    );
}
