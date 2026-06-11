//! M1 integration suite: every intercepted-in-the-future endpoint must be
//! byte-identical through the proxy today, in both JSON and XML.
//!
//! Requires the harness Navidrome: `tests/harness/up.sh`, then
//! `cargo test --test passthrough -- --ignored`.

// The serial() guard is deliberately held across awaits: every #[tokio::test]
// runs on its own thread with its own runtime, so blocking the thread is the
// intended cross-test serialization and cannot deadlock.
#![allow(clippy::await_holding_lock)]

mod common;

use common::*;
use reqwest::StatusCode;

/// Fetch `path_and_query` from Navidrome directly and through a fresh
/// in-process proxy; assert status, content-type and body are identical.
async fn assert_passthrough_identical(path_and_query: &str) {
    let navidrome = navidrome_url();
    let proxy = spawn_proxy(&navidrome).await;
    ensure_library_scanned(&navidrome).await;

    let (direct_status, direct_headers, direct_body) = fetch(&navidrome, path_and_query).await;
    let (proxy_status, proxy_headers, proxy_body) = fetch(&proxy, path_and_query).await;

    assert_eq!(
        direct_status, proxy_status,
        "status differs: {path_and_query}"
    );
    assert_eq!(
        direct_headers.get("content-type"),
        proxy_headers.get("content-type"),
        "content-type differs: {path_and_query}"
    );
    // normalize_body only canonicalizes Navidrome's nondeterministic
    // `roles` ordering; any proxy-introduced change still fails.
    assert_eq!(
        normalize_body(&direct_body),
        normalize_body(&proxy_body),
        "body differs ({} vs {} bytes): {path_and_query}",
        direct_body.len(),
        proxy_body.len()
    );
}

/// Run an endpoint comparison in both response formats (JSON + default XML).
async fn assert_both_formats(endpoint: &str, extra: &str) {
    for format in [Some("json"), None] {
        let auth = auth_query(format);
        let sep = if extra.is_empty() { "" } else { "&" };
        assert_passthrough_identical(&format!("/rest/{endpoint}?{auth}{sep}{extra}")).await;
    }
}

/// First seeded album (alphabetical), as (album_id, cover_art_id). Resolved
/// via browsing endpoints, not search3 — Navidrome's search index can lag
/// behind scan/scrobble activity, while browsing reads the library directly.
async fn seeded_album(navidrome: &str) -> (String, String) {
    let auth = auth_query(Some("json"));
    let body = fetch_json(
        navidrome,
        &format!("/rest/getAlbumList2?{auth}&type=alphabeticalByName&size=5"),
    )
    .await;
    let album = &body["subsonic-response"]["albumList2"]["album"][0];
    (
        album["id"].as_str().expect("no seeded album").to_string(),
        album["coverArt"]
            .as_str()
            .expect("seeded album has no cover")
            .to_string(),
    )
}

async fn seeded_song_id(navidrome: &str) -> String {
    let (album_id, _) = seeded_album(navidrome).await;
    let auth = auth_query(Some("json"));
    let body = fetch_json(navidrome, &format!("/rest/getAlbum?{auth}&id={album_id}")).await;
    body["subsonic-response"]["album"]["song"][0]["id"]
        .as_str()
        .expect("seeded album has no songs")
        .to_string()
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn ping() {
    let _guard = serial();
    assert_both_formats("ping", "").await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn get_license() {
    let _guard = serial();
    assert_both_formats("getLicense", "").await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn get_artists() {
    let _guard = serial();
    assert_both_formats("getArtists", "").await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn get_album_list2() {
    let _guard = serial();
    assert_both_formats("getAlbumList2", "type=alphabeticalByName&size=50").await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn search3() {
    let _guard = serial();
    assert_both_formats(
        "search3",
        "query=Tone&songCount=20&albumCount=5&artistCount=5",
    )
    .await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn get_cover_art() {
    let _guard = serial();
    let navidrome = navidrome_url();
    ensure_library_scanned(&navidrome).await;
    let (_, cover) = seeded_album(&navidrome).await;
    let auth = auth_query(None);
    assert_passthrough_identical(&format!("/rest/getCoverArt?{auth}&id={cover}")).await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn stream_bytes_match() {
    let _guard = serial();
    let navidrome = navidrome_url();
    ensure_library_scanned(&navidrome).await;
    let song = seeded_song_id(&navidrome).await;
    // format=raw: no transcoding, so both responses are the original file.
    let auth = auth_query(None);
    assert_passthrough_identical(&format!("/rest/stream?{auth}&id={song}&format=raw")).await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn scrobble() {
    let _guard = serial();
    let navidrome = navidrome_url();
    ensure_library_scanned(&navidrome).await;
    let song = seeded_song_id(&navidrome).await;
    assert_both_formats("scrobble", &format!("id={song}&submission=true")).await;
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn web_ui_and_root_redirect() {
    let _guard = serial();
    let navidrome = navidrome_url();
    let proxy = spawn_proxy(&navidrome).await;

    // Root: Navidrome 302-redirects to /app/ — the proxy must pass the
    // redirect through untouched, not follow it.
    let (direct_status, direct_headers, _) = fetch(&navidrome, "/").await;
    let (proxy_status, proxy_headers, _) = fetch(&proxy, "/").await;
    assert_eq!(direct_status, proxy_status);
    assert_eq!(
        direct_headers.get("location"),
        proxy_headers.get("location")
    );

    // Web UI shell is served identically.
    assert_passthrough_identical("/app/").await;
}

// ---- No harness required below (pure proxy behavior) ----

#[tokio::test]
async fn upstream_down_returns_502() {
    // Port 9 (discard) refuses connections immediately.
    let proxy = spawn_proxy("http://127.0.0.1:9").await;
    let (status, _, body) = fetch(&proxy, "/rest/ping").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(&body[..], b"upstream unavailable");
}

#[tokio::test]
async fn healthz_does_not_proxy() {
    let proxy = spawn_proxy("http://127.0.0.1:9").await;
    let (status, _, body) = fetch(&proxy, "/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..], b"ok");
}
