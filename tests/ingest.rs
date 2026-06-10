//! M4 integration suite: staged stream → tagged file in the library →
//! Navidrome scan → id remap → scrobble replay. Mock yt-dlp + mock Deezer,
//! real Navidrome (harness).
//!
//! Requires `tests/harness/up.sh`, then
//! `cargo test --test ingest -- --ignored --test-threads=1`.

#![allow(clippy::await_holding_lock)]

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::*;
use reqwest::StatusCode;
use songarr_proxy::config::Config;

fn music_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/harness/data/music")
}

fn mock_ytdlp() -> String {
    format!("{}/tests/harness/bin/yt-dlp", env!("CARGO_MANIFEST_DIR"))
}

async fn spawn_m4_proxy(deezer: &str) -> TestProxy {
    let deezer = deezer.to_string();
    spawn_proxy_with_worker(&navidrome_url(), move |config: &mut Config| {
        config.external_search.api_base_deezer = deezer;
        config.streaming.ytdlp_path = mock_ytdlp();
        config.library.music_dir = music_dir();
        config.ingest.poll_secs = 1;
    })
    .await
}

/// Remove leftovers from previous runs so Navidrome search results (and
/// the M1/M2 byte-compare suites) stay deterministic.
async fn cleanup_imports() {
    let ingest_dir = music_dir().join("_songarr");
    let playlist = music_dir().join("Songarr Played.m3u");
    let had_leftovers = ingest_dir.exists() || playlist.exists();
    let _ = std::fs::remove_dir_all(&ingest_dir);
    let _ = std::fs::remove_file(&playlist);
    if !had_leftovers {
        return;
    }
    // Rescan so Navidrome forgets the deleted files.
    let navidrome = navidrome_url();
    let auth = auth_query(Some("json"));
    fetch(&navidrome, &format!("/rest/startScan?{auth}")).await;
    for _ in 0..60 {
        let body = fetch_json(&navidrome, &format!("/rest/getScanStatus?{auth}")).await;
        if body["subsonic-response"]["scanStatus"]["scanning"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn virtual_id(proxy_url: &str, query: &str, title: &str) -> String {
    let body = fetch_json(
        proxy_url,
        &format!("/rest/search3?{}&query={query}", auth_query(Some("json"))),
    )
    .await;
    body["subsonic-response"]["searchResult3"]["song"]
        .as_array()
        .expect("song array")
        .iter()
        .find(|s| s["title"] == title && s["id"].as_str().unwrap_or("").starts_with("sgr_"))
        .map(|s| s["id"].as_str().unwrap().to_string())
        .unwrap_or_else(|| panic!("virtual '{title}' not injected: {body}"))
}

async fn wait_for_track_status(proxy: &TestProxy, id: &str, status: &str) {
    let pool = proxy.db().await;
    for _ in 0..300 {
        let current: (String,) =
            sqlx::query_as("SELECT status FROM virtual_tracks WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        if current.0 == status {
            pool.close().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let debug: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, status, error FROM stream_jobs")
            .fetch_all(&pool)
            .await
            .unwrap();
    panic!("track {id} never reached {status:?}; jobs: {debug:?}");
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn full_import_flow_end_to_end() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    cleanup_imports().await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m4_proxy(&deezer).await;
    let id = virtual_id(&proxy.url, "Mock+Artist", "Mock Song One").await;

    // Scrobble BEFORE the import exists — must be queued, then replayed.
    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/scrobble?{}&id={id}&submission=true",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");

    // Play it (full live pipe).
    let (status, _, audio) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(audio.starts_with(b"OggS"));

    // The worker should take it all the way to imported.
    wait_for_track_status(&proxy, &id, "imported").await;

    let pool = proxy.db().await;
    let (real_id,): (String,) =
        sqlx::query_as("SELECT real_subsonic_id FROM virtual_tracks WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!real_id.is_empty());

    // File landed in the plan's layout, tagged file is the opus remux.
    let expected = music_dir().join("_songarr/Mock Artist/Mock Artist - Mock Song One.opus");
    assert!(expected.exists(), "missing {expected:?}");

    // The track is now a first-class Navidrome citizen (direct, no proxy).
    let direct = fetch_json(
        &navidrome_url(),
        &format!("/rest/getSong?{}&id={real_id}", auth_query(Some("json"))),
    )
    .await;
    assert_eq!(direct["subsonic-response"]["status"], "ok");
    let song = &direct["subsonic-response"]["song"];
    assert_eq!(song["artist"], "Mock Artist", "{song}");
    assert_eq!(song["title"], "Mock Song One", "{song}");
    // Queued scrobble was replayed as the original user.
    assert!(
        song["playCount"].as_i64().unwrap_or(0) >= 1,
        "scrobble must be replayed: {song}"
    );

    // Streaming the OLD virtual id now passes through to the real file:
    // finite (Content-Length) instead of a live chunked pipe.
    let (status, headers, _) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}&format=raw", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers.get("content-length").is_some(),
        "imported track must be served by Navidrome (seekable)"
    );

    // getSong on the virtual id also serves Navidrome's metadata now.
    let via_virtual = fetch_json(
        &proxy.url,
        &format!("/rest/getSong?{}&id={id}", auth_query(Some("json"))),
    )
    .await;
    assert_eq!(
        via_virtual["subsonic-response"]["song"]["id"].as_str().unwrap(),
        real_id
    );

    // Searching again shows the real track, no sgr_ duplicate.
    let results = fetch_json(
        &proxy.url,
        &format!("/rest/search3?{}&query=Mock+Artist", auth_query(Some("json"))),
    )
    .await;
    let songs = results["subsonic-response"]["searchResult3"]["song"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mock_ones: Vec<_> = songs.iter().filter(|s| s["title"] == "Mock Song One").collect();
    assert_eq!(mock_ones.len(), 1, "exactly one Mock Song One: {results}");
    assert!(
        !mock_ones[0]["id"].as_str().unwrap().starts_with("sgr_"),
        "the real import wins over re-injection"
    );

    // Rolling playlist recorded the import.
    let playlist = std::fs::read_to_string(music_dir().join("Songarr Played.m3u")).unwrap();
    assert!(playlist.contains("Mock Artist - Mock Song One"), "{playlist}");

    cleanup_imports().await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn low_match_score_parks_in_needs_review() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    cleanup_imports().await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m4_proxy(&deezer).await;
    // Provider says 999s, mock yt-dlp candidates say 180s → low score.
    let id = virtual_id(&proxy.url, "Mock+Song+Low", "Mock Song Low").await;

    let (status, _, audio) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "low score must still stream");
    assert!(audio.starts_with(b"OggS"));

    // Worker parks it instead of importing.
    let pool = proxy.db().await;
    let mut parked: Option<(String, Option<String>, Option<String>)> = None;
    for _ in 0..60 {
        let job: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT status, error, staging_path FROM stream_jobs WHERE virtual_track_id = ?",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
        if job.0 == "needs_review" {
            parked = Some(job);
            break;
        }
        assert_ne!(job.0, "imported", "low score must never auto-import");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let (_, error, staging_path) = parked.expect("job parked in needs_review");
    assert!(error.unwrap_or_default().contains("below threshold"));
    // File stays in staging for the M5 review queue.
    assert!(std::fs::metadata(staging_path.unwrap()).unwrap().len() > 0);

    // Nothing imported.
    let (real_id,): (Option<String>,) =
        sqlx::query_as("SELECT real_subsonic_id FROM virtual_tracks WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(real_id.is_none());
    assert!(!music_dir()
        .join("_songarr/Mock Artist/Mock Artist - Mock Song Low.opus")
        .exists());
}
