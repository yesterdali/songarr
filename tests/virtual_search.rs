//! M2 integration suite: virtual track injection against the harness
//! Navidrome with a mock Deezer API (no real external traffic).
//!
//! Requires `tests/harness/up.sh`, then
//! `cargo test --test virtual_search -- --ignored`.

#![allow(clippy::await_holding_lock)]

mod common;

use std::sync::atomic::Ordering;

use common::*;
use reqwest::StatusCode;

/// Proxy wired to harness Navidrome + mock Deezer.
async fn spawn_m2_proxy(deezer_base: &str) -> String {
    let deezer_base = deezer_base.to_string();
    spawn_proxy_with(&navidrome_url(), move |config| {
        config.external_search.api_base_deezer = deezer_base;
    })
    .await
}

fn songs_of(body: &serde_json::Value) -> Vec<serde_json::Value> {
    body["subsonic-response"]["searchResult3"]["song"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn assert_valid_xml(body: &str) {
    let mut reader = quick_xml::Reader::from_str(body);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(e) => panic!("invalid xml: {e}\n{body}"),
        }
    }
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn search3_appends_virtual_songs_json_and_xml() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m2_proxy(&deezer).await;

    // JSON: local songs first, then virtual ones with full synthesized fields.
    let body = fetch_json(
        &proxy,
        &format!("/rest/search3?{}&query=Mock+Artist", auth_query(Some("json"))),
    )
    .await;
    let songs = songs_of(&body);
    let virtual_songs: Vec<_> = songs
        .iter()
        .filter(|s| s["id"].as_str().unwrap_or("").starts_with("sgr_"))
        .collect();
    assert_eq!(virtual_songs.len(), 2, "both mock tracks injected: {body}");
    let mock_one = virtual_songs
        .iter()
        .find(|s| s["title"] == "Mock Song One")
        .expect("Mock Song One present");
    assert_eq!(mock_one["artist"], "Mock Artist");
    assert_eq!(mock_one["album"], "Mock Album");
    assert_eq!(mock_one["duration"], 180);
    assert_eq!(mock_one["suffix"], "opus");
    assert_eq!(mock_one["contentType"], "audio/ogg");
    assert_eq!(mock_one["coverArt"], mock_one["id"]);
    assert!(mock_one["size"].as_i64().unwrap() > 0);
    // Envelope preserved from Navidrome.
    assert!(body["subsonic-response"]["serverVersion"]
        .as_str()
        .unwrap()
        .starts_with("0."));

    // XML: same injection, well-formed, virtual entries present.
    let (status, headers, raw) = fetch(
        &proxy,
        &format!("/rest/search3?{}&query=Mock+Artist", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "application/xml"
    );
    let xml = String::from_utf8(raw.to_vec()).unwrap();
    assert_valid_xml(&xml);
    assert!(xml.contains("sgr_"), "{xml}");
    assert!(xml.contains(r#"title="Mock Song One""#), "{xml}");
    assert!(xml.contains("</subsonic-response>"), "{xml}");
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn virtual_ids_are_stable_across_searches() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m2_proxy(&deezer).await;

    let mut ids = Vec::new();
    for _ in 0..2 {
        let body = fetch_json(
            &proxy,
            &format!("/rest/search3?{}&query=Mock+Artist", auth_query(Some("json"))),
        )
        .await;
        let id = songs_of(&body)
            .iter()
            .find(|s| s["title"] == "Mock Song One")
            .map(|s| s["id"].as_str().unwrap().to_string())
            .expect("virtual song present");
        ids.push(id);
    }
    assert_eq!(ids[0], ids[1], "same provider track must keep its sgr_ id");
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn local_tracks_are_not_duplicated() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m2_proxy(&deezer).await;

    // "Tone" returns the real seeded "Tone 220 Hz"; the mock also offers it.
    let body = fetch_json(
        &proxy,
        &format!("/rest/search3?{}&query=Tone&songCount=30", auth_query(Some("json"))),
    )
    .await;
    let songs = songs_of(&body);
    let tone_entries: Vec<_> = songs
        .iter()
        .filter(|s| s["title"] == "Tone 220 Hz")
        .collect();
    assert_eq!(tone_entries.len(), 1, "no duplicate of a local track: {body}");
    assert!(
        !tone_entries[0]["id"].as_str().unwrap().starts_with("sgr_"),
        "the local track wins"
    );
    // The non-colliding mock track still got injected.
    assert!(
        songs.iter().any(|s| s["title"] == "Mock Song One"),
        "{body}"
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn denied_users_and_short_queries_get_vanilla_results() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;

    let deezer_clone = deezer.clone();
    let denying_proxy = spawn_proxy_with(&navidrome_url(), move |config| {
        config.external_search.api_base_deezer = deezer_clone;
        config.users.deny = vec![USER.to_string()];
    })
    .await;
    let body = fetch_json(
        &denying_proxy,
        &format!("/rest/search3?{}&query=Mock+Artist", auth_query(Some("json"))),
    )
    .await;
    assert!(
        songs_of(&body).iter().all(|s| !s["id"].as_str().unwrap_or("").starts_with("sgr_")),
        "denied user must see vanilla results: {body}"
    );

    let proxy = spawn_m2_proxy(&deezer).await;
    let body = fetch_json(
        &proxy,
        &format!("/rest/search3?{}&query=To", auth_query(Some("json"))),
    )
    .await;
    assert!(
        songs_of(&body).iter().all(|s| !s["id"].as_str().unwrap_or("").starts_with("sgr_")),
        "below min_query_len no injection happens: {body}"
    );
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn get_song_synthesizes_virtual_and_errors_on_unknown() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, _) = spawn_mock_deezer().await;
    let proxy = spawn_m2_proxy(&deezer).await;

    let body = fetch_json(
        &proxy,
        &format!("/rest/search3?{}&query=Mock+Artist", auth_query(Some("json"))),
    )
    .await;
    let virtual_id = songs_of(&body)
        .iter()
        .find(|s| s["title"] == "Mock Song One")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .expect("virtual song present");

    // JSON
    let body = fetch_json(
        &proxy,
        &format!("/rest/getSong?{}&id={virtual_id}", auth_query(Some("json"))),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");
    let song = &body["subsonic-response"]["song"];
    assert_eq!(song["id"], virtual_id.as_str());
    assert_eq!(song["title"], "Mock Song One");
    assert_eq!(song["duration"], 180);
    // Envelope mirrors Navidrome.
    assert_eq!(body["subsonic-response"]["type"], "navidrome");

    // XML
    let (status, _, raw) = fetch(
        &proxy,
        &format!("/rest/getSong?{}&id={virtual_id}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(raw.to_vec()).unwrap();
    assert_valid_xml(&xml);
    assert!(xml.contains(r#"status="ok""#), "{xml}");
    assert!(xml.contains(&format!(r#"id="{virtual_id}""#)), "{xml}");

    // Unknown virtual id → well-formed subsonic error 70, never a 500.
    let body = fetch_json(
        &proxy,
        &format!(
            "/rest/getSong?{}&id=sgr_0000000000000000000000",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert_eq!(body["subsonic-response"]["error"]["code"], 70);
}

#[tokio::test]
#[ignore = "integration: run tests/harness/up.sh first"]
async fn get_cover_art_serves_and_caches_virtual_artwork() {
    let _guard = serial();
    ensure_library_scanned(&navidrome_url()).await;
    let (deezer, art_hits) = spawn_mock_deezer().await;
    let proxy = spawn_m2_proxy(&deezer).await;

    let body = fetch_json(
        &proxy,
        &format!("/rest/search3?{}&query=Mock+Artist", auth_query(Some("json"))),
    )
    .await;
    let cover_id = songs_of(&body)
        .iter()
        .find(|s| s["title"] == "Mock Song One")
        .map(|s| s["coverArt"].as_str().unwrap().to_string())
        .expect("virtual song with coverArt");

    for round in 1..=2 {
        let (status, headers, raw) = fetch(
            &proxy,
            &format!("/rest/getCoverArt?{}&id={cover_id}", auth_query(None)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "round {round}");
        assert_eq!(
            headers.get("content-type").unwrap().to_str().unwrap(),
            "image/jpeg"
        );
        assert_eq!(&raw[..], FAKE_JPEG, "round {round}");
    }
    assert_eq!(
        art_hits.load(Ordering::SeqCst),
        1,
        "second hit must come from the disk cache"
    );

    // Real cover ids keep passing through to Navidrome untouched.
    let (_, real_cover) = seeded_album_via(&proxy).await;
    let (status, _, raw) = fetch(
        &proxy,
        &format!("/rest/getCoverArt?{}&id={real_cover}", auth_query(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(raw.len() > FAKE_JPEG.len(), "real art is a real jpeg");
}

/// Resolve the first seeded album through any base URL (proxy or direct).
async fn seeded_album_via(base: &str) -> (String, String) {
    let auth = auth_query(Some("json"));
    let body = fetch_json(
        base,
        &format!("/rest/getAlbumList2?{auth}&type=alphabeticalByName&size=5"),
    )
    .await;
    let album = &body["subsonic-response"]["albumList2"]["album"][0];
    (
        album["id"].as_str().expect("album id").to_string(),
        album["coverArt"].as_str().expect("album cover").to_string(),
    )
}
