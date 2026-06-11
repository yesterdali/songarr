//! R1 recommendation integration tests: mock Navidrome + mock YTM, no real
//! external services and no Docker harness required.

#![allow(clippy::await_holding_lock)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::Query;
use axum::routing::{get, post};
use axum::{Json, Router};
use common::*;
use reqwest::StatusCode;
use serde_json::json;
use songarr_proxy::config::Config;
use songarr_proxy::vtrack::{self, CatalogTrack};

fn mock_ytdlp() -> String {
    format!("{}/tests/harness/bin/yt-dlp", env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/harness/data/fixtures/source.webm")
}

async fn spawn_r1_proxy(upstream: &str, ytm_base: String) -> TestProxy {
    spawn_rec_proxy(upstream, ytm_base, |_| {}).await
}

async fn spawn_rec_proxy(
    upstream: &str,
    ytm_base: String,
    customize: impl FnOnce(&mut Config),
) -> TestProxy {
    spawn_proxy_handle(upstream, move |config| {
        config.external_search.enabled = false;
        config.recommendations.enabled = true;
        config.recommendations.max_results = 5;
        config.recommendations.shown_cooldown_days = 0;
        config.recommendations.cache_ttl_hours = 72;
        config.recommendations.weight_deezer = 0.0;
        config.recommendations.weight_lastfm = 0.0;
        config.recommendations.ytm_api_base = ytm_base;
        config.streaming.ytdlp_path = mock_ytdlp();
        customize(config);
    })
    .await
}

async fn spawn_mock_navidrome() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/rest/ping", get(ping))
        .route("/rest/getSong", get(get_song))
        .route("/rest/getSimilarSongs", get(similar_songs_xml))
        .route("/rest/getSimilarSongs.view", get(similar_songs_xml))
        .route("/rest/getSimilarSongs2", get(similar_songs_json))
        .route("/rest/getSimilarSongs2.view", get(similar_songs_json))
        .route("/rest/getTopSongs", get(top_songs_json))
        .route("/rest/getTopSongs.view", get(top_songs_json))
        .route("/rest/getPlaylists", get(playlists_json))
        .route("/rest/getPlaylists.view", get(playlists_json))
        .route("/rest/getPlaylist", get(upstream_playlist_json))
        .route("/rest/getPlaylist.view", get(upstream_playlist_json))
        .route("/rest/scrobble", get(scrobble_ok))
        .route("/rest/scrobble.view", get(scrobble_ok));
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    base
}

async fn ping() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true
        }
    }))
}

async fn get_song(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let id = params.get("id").map(String::as_str).unwrap_or("");
    let song = if id == "real_seed" {
        json!({"id": "real_seed", "title": "Seed Song", "artist": "Seed Artist", "duration": 180})
    } else {
        json!({"id": id, "title": "Other Song", "artist": "Other Artist", "duration": 200})
    };
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "song": song
        }
    }))
}

async fn similar_songs_json() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "similarSongs2": {
                "song": [{
                    "id": "local_dup",
                    "title": "Local Existing",
                    "artist": "Mock Radio",
                    "duration": 180
                }]
            }
        }
    }))
}

async fn similar_songs_xml() -> ([(&'static str, &'static str); 1], &'static str) {
    (
        [("content-type", "application/xml")],
        r#"<?xml version="1.0" encoding="UTF-8"?><subsonic-response xmlns="http://subsonic.org/restapi" status="ok" version="1.16.1" type="navidrome" serverVersion="0.62.0-test" openSubsonic="true"><similarSongs><song id="local_dup" title="Local Existing" artist="Mock Radio" duration="180"/></similarSongs></subsonic-response>"#,
    )
}

async fn top_songs_json() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "topSongs": {"song": []}
        }
    }))
}

async fn playlists_json() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "playlists": {
                "playlist": [{
                    "id": "existing_playlist",
                    "name": "Existing",
                    "owner": USER,
                    "public": false,
                    "songCount": 0,
                    "duration": 0,
                    "created": "2026-01-01T00:00:00Z",
                    "changed": "2026-01-01T00:00:00Z"
                }]
            }
        }
    }))
}

async fn upstream_playlist_json() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "playlist": {
                "id": "existing_playlist",
                "name": "Existing",
                "owner": USER,
                "public": false,
                "songCount": 0,
                "duration": 0,
                "entry": []
            }
        }
    }))
}

async fn scrobble_ok() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true
        }
    }))
}

async fn spawn_mock_ytm() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/youtubei/v1/next", post(ytm_next_assert_radio))
        .route("/youtubei/v1/search", post(ytm_search));
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    base
}

async fn ytm_next() -> Json<serde_json::Value> {
    Json(json!({
        "contents": {
            "singleColumnMusicWatchNextResultsRenderer": {
                "playlistPanelRenderer": {
                    "contents": [
                        panel_video("seedvideo01", "Seed Song", "Seed Artist", "3:00"),
                        panel_video("localdup01", "Local Existing", "Mock Radio", "3:00"),
                        panel_video("radionew01", "Radio New", "Mock Radio", "3:12")
                    ]
                }
            }
        }
    }))
}

async fn ytm_next_assert_radio(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let video_id = body["videoId"].as_str().expect("videoId");
    assert_eq!(
        body["playlistId"],
        format!("RDAMVM{video_id}"),
        "radio requests must ask YTM for the automix playlist"
    );
    ytm_next().await
}

fn panel_video(id: &str, title: &str, artist: &str, length: &str) -> serde_json::Value {
    json!({
        "playlistPanelVideoRenderer": {
            "videoId": id,
            "title": {"runs": [{"text": title}]},
            "longBylineText": {"runs": [
                {"text": artist},
                {"text": " • "},
                {"text": "Album"}
            ]},
            "lengthText": {"runs": [{"text": length}]}
        }
    })
}

async fn ytm_search() -> Json<serde_json::Value> {
    Json(json!({
        "contents": [{
            "musicResponsiveListItemRenderer": {
                "flexColumns": [
                    {"musicResponsiveListItemFlexColumnRenderer": {
                        "text": {"runs": [{"text": "Top Hit"}]}
                    }},
                    {"musicResponsiveListItemFlexColumnRenderer": {
                        "text": {"runs": [
                            {"text": "Mock Artist"},
                            {"text": " • "},
                            {"text": "Song"}
                        ]}
                    }}
                ],
                "fixedColumns": [{
                    "musicResponsiveListItemFixedColumnRenderer": {
                        "text": {"runs": [{"text": "2:45"}]}
                    }
                }],
                "playlistItemData": {"videoId": "tophit01"}
            }
        }]
    }))
}

async fn spawn_mock_deezer_recs() -> (String, Arc<AtomicUsize>) {
    use axum::routing::get;

    let hits = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hit_counter = hits.clone();
    let router = Router::new().route(
        "/search",
        get(
            move |Query(params): Query<std::collections::HashMap<String, String>>| {
                let hits = hit_counter.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    let q = params.get("q").cloned().unwrap_or_default();
                    let data = if q.contains("Seed Artist") || q.contains("Mock Radio") {
                        json!([
                            deezer_track(400, "Radio New", "Mock Radio", 192),
                            deezer_track(401, "Deezer Solo", "Mock Radio", 201)
                        ])
                    } else if q.contains("Mock Artist") {
                        json!([
                            deezer_track(500, "Top Hit", "Mock Artist", 165),
                            deezer_track(501, "Deezer Top", "Mock Artist", 188)
                        ])
                    } else {
                        json!([])
                    };
                    let total = data.as_array().map(Vec::len).unwrap_or(0);
                    Json(json!({"data": data, "total": total}))
                }
            },
        ),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (base, hits)
}

fn deezer_track(id: u64, title: &str, artist: &str, duration: i64) -> serde_json::Value {
    json!({
        "id": id,
        "title": title,
        "duration": duration,
        "artist": {"name": artist},
        "album": {
            "title": "Mock Album",
            "cover_big": format!("https://img.example/{id}.jpg")
        },
        "type": "track"
    })
}

async fn spawn_mock_lastfm() -> (String, Arc<AtomicUsize>) {
    use axum::routing::get;

    let hits = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hit_counter = hits.clone();
    let router = Router::new().route(
        "/2.0",
        get(move |Query(params): Query<std::collections::HashMap<String, String>>| {
            let hits = hit_counter.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                let method = params.get("method").map(String::as_str).unwrap_or("");
                let body = match method {
                    "track.getSimilar" => json!({
                        "similartracks": {"track": [
                            {"name": "Radio New", "artist": {"name": "Mock Radio"}, "duration": "192"},
                            {"name": "Lastfm Solo", "artist": {"name": "Mock Radio"}, "duration": "205"}
                        ]}
                    }),
                    "artist.getTopTracks" => json!({
                        "toptracks": {"track": [
                            {"name": "Top Hit", "artist": {"name": "Mock Artist"}, "duration": "165"},
                            {"name": "Lastfm Top", "artist": {"name": "Mock Artist"}, "duration": "199"}
                        ]}
                    }),
                    _ => json!({}),
                };
                Json(body)
            }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (base, hits)
}

async fn insert_virtual_seed(proxy: &TestProxy) -> String {
    let pool = proxy.db().await;
    let id = vtrack::upsert(
        &pool,
        &CatalogTrack {
            provider: "ytmusic",
            provider_track_id: "seedvideo01".into(),
            artist: "Seed Artist".into(),
            title: "Seed Song".into(),
            album: None,
            duration_ms: Some(180_000),
            isrc: None,
            artwork_url: None,
        },
    )
    .await
    .unwrap();
    vtrack::set_resolution(
        &pool,
        &id,
        "https://www.youtube.com/watch?v=seedvideo01",
        100,
        "Seed Song",
    )
    .await
    .unwrap();
    id
}

fn similar2_songs(body: &serde_json::Value) -> Vec<serde_json::Value> {
    body["subsonic-response"]["similarSongs2"]["song"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn virtual_seed_radio_returns_playable_preresolved_tracks() {
    let _guard = serial();
    assert!(
        fixture_path().exists(),
        "fixture missing — run tests/harness/seed.sh"
    );
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let proxy = spawn_r1_proxy(&upstream, ytm).await;
    let seed_id = insert_virtual_seed(&proxy).await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getSimilarSongs2?{}&id={seed_id}&count=3",
            auth_query(Some("json"))
        ),
    )
    .await;
    let songs = similar2_songs(&body);
    let radio_new = songs
        .iter()
        .find(|s| s["title"] == "Radio New")
        .expect("radio recommendation present");
    let rec_id = radio_new["id"].as_str().unwrap();
    assert!(rec_id.starts_with("sgr_"));

    let pool = proxy.db().await;
    let (resolved_url,): (String,) =
        sqlx::query_as("SELECT resolved_url FROM virtual_tracks WHERE id = ?")
            .bind(rec_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(resolved_url.ends_with("v=radionew01"), "{resolved_url}");

    let log_path = std::env::temp_dir().join(format!("ytdlp-r1-{}.log", uuid::Uuid::new_v4()));
    std::env::set_var("MOCK_YTDLP_LOG", &log_path);
    let (status, headers, body) = fetch(
        &proxy.url,
        &format!("/rest/stream?{}&id={rec_id}", auth_query(None)),
    )
    .await;
    std::env::remove_var("MOCK_YTDLP_LOG");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "audio/ogg");
    assert!(body.starts_with(b"OggS"));
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        log.lines().all(|line| !line.contains("ytsearch")),
        "recommended track should play from pre-resolved video id:\n{log}"
    );
}

#[tokio::test]
async fn real_seed_radio_merges_with_navidrome_and_dedups() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let proxy = spawn_r1_proxy(&upstream, ytm).await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getSimilarSongs2?{}&id=real_seed&count=5",
            auth_query(Some("json"))
        ),
    )
    .await;
    let songs = similar2_songs(&body);
    assert_eq!(
        songs
            .iter()
            .filter(|s| s["title"] == "Local Existing")
            .count(),
        1,
        "Navidrome local entry must not be duplicated: {body}"
    );
    assert!(
        songs.iter().any(
            |s| s["title"] == "Radio New" && s["id"].as_str().unwrap_or("").starts_with("sgr_")
        ),
        "YTM recommendation should be appended: {body}"
    );

    let (status, _, raw) = fetch(
        &proxy.url,
        &format!(
            "/rest/getSimilarSongs?{}&id=real_seed&count=5",
            auth_query(None)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(raw.to_vec()).unwrap();
    assert!(xml.contains("<similarSongs>"), "{xml}");
    assert!(xml.contains(r#"title="Radio New""#), "{xml}");
    assert!(xml.contains(r#"id="sgr_"#) || xml.contains("sgr_"), "{xml}");
}

#[tokio::test]
async fn top_songs_uses_ytm_search() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let proxy = spawn_r1_proxy(&upstream, ytm).await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getTopSongs?{}&artist=Mock+Artist&count=3",
            auth_query(Some("json"))
        ),
    )
    .await;
    let songs = body["subsonic-response"]["topSongs"]["song"]
        .as_array()
        .unwrap();
    assert_eq!(songs.len(), 1, "{body}");
    assert_eq!(songs[0]["title"], "Top Hit");
    assert_eq!(songs[0]["artist"], "Mock Artist");
    assert_eq!(songs[0]["duration"], 165);
    assert!(songs[0]["id"].as_str().unwrap().starts_with("sgr_"));
}

#[tokio::test]
async fn r2_ensemble_consensus_wins_and_lastfm_is_cached() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let (deezer, _deezer_hits) = spawn_mock_deezer_recs().await;
    let (lastfm, lastfm_hits) = spawn_mock_lastfm().await;
    let proxy = spawn_rec_proxy(&upstream, ytm, move |config| {
        config.external_search.api_base_deezer = deezer;
        config.recommendations.lastfm_api_base = format!("{lastfm}/2.0");
        config.recommendations.lastfm_api_key = "test-key".into();
        config.recommendations.weight_ytm = 1.0;
        config.recommendations.weight_deezer = 0.8;
        config.recommendations.weight_lastfm = 0.8;
        config.recommendations.shown_cooldown_days = 0;
    })
    .await;

    let path = format!(
        "/rest/getSimilarSongs2?{}&id=real_seed&count=5",
        auth_query(Some("json"))
    );
    let body = fetch_json(&proxy.url, &path).await;
    let songs = similar2_songs(&body);
    let virtual_songs: Vec<_> = songs
        .iter()
        .filter(|s| s["id"].as_str().unwrap_or("").starts_with("sgr_"))
        .collect();
    assert!(
        virtual_songs.len() >= 3,
        "ensemble should append several virtual candidates: {body}"
    );
    assert_eq!(
        virtual_songs[0]["title"], "Radio New",
        "overlap from YTM + Deezer + Last.fm should rank first: {body}"
    );
    assert_eq!(virtual_songs[0]["album"], "Mock Album");
    assert_eq!(virtual_songs[0]["coverArt"], virtual_songs[0]["id"]);

    let lastfm_after_first = lastfm_hits.load(Ordering::SeqCst);
    let _ = fetch_json(&proxy.url, &path).await;
    assert_eq!(
        lastfm_hits.load(Ordering::SeqCst),
        lastfm_after_first,
        "second identical request should use rec_cache for Last.fm"
    );

    let pool = proxy.db().await;
    let cached: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM rec_cache WHERE source LIKE 'lastfm_%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(cached.0 >= 1, "Last.fm response should be cached");
}

#[tokio::test]
async fn r2_top_songs_ensemble_uses_multiple_sources() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let (deezer, _) = spawn_mock_deezer_recs().await;
    let (lastfm, _) = spawn_mock_lastfm().await;
    let proxy = spawn_rec_proxy(&upstream, ytm, move |config| {
        config.external_search.api_base_deezer = deezer;
        config.recommendations.lastfm_api_base = format!("{lastfm}/2.0");
        config.recommendations.lastfm_api_key = "test-key".into();
        config.recommendations.weight_deezer = 0.8;
        config.recommendations.weight_lastfm = 0.8;
    })
    .await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getTopSongs?{}&artist=Mock+Artist&count=5",
            auth_query(Some("json"))
        ),
    )
    .await;
    let songs = body["subsonic-response"]["topSongs"]["song"]
        .as_array()
        .unwrap();
    assert_eq!(
        songs[0]["title"], "Top Hit",
        "Top Hit appears in all sources and should win: {body}"
    );
    assert!(
        songs
            .iter()
            .any(|song| song["title"] == "Deezer Top" || song["title"] == "Lastfm Top"),
        "single-source candidates should still survive below consensus winners: {body}"
    );
}

#[tokio::test]
async fn r2_recently_shown_candidates_are_suppressed() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let proxy = spawn_rec_proxy(&upstream, ytm, |config| {
        config.recommendations.shown_cooldown_days = 7;
    })
    .await;
    let seed_id = insert_virtual_seed(&proxy).await;
    let path = format!(
        "/rest/getSimilarSongs2?{}&id={seed_id}&count=3",
        auth_query(Some("json"))
    );

    let first = fetch_json(&proxy.url, &path).await;
    assert!(
        similar2_songs(&first)
            .iter()
            .any(|s| s["title"] == "Radio New"),
        "first request should show Radio New: {first}"
    );
    let second = fetch_json(&proxy.url, &path).await;
    assert!(
        similar2_songs(&second)
            .iter()
            .all(|s| s["title"] != "Radio New"),
        "second request inside cooldown should suppress previously shown candidates: {second}"
    );
}

#[tokio::test]
async fn r3_scrobbles_are_logged_for_virtual_and_real_tracks() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let proxy = spawn_r1_proxy(&upstream, ytm).await;
    let seed_id = insert_virtual_seed(&proxy).await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/scrobble?{}&id={seed_id}&time=1700000000123",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/scrobble?{}&id=real_seed&time=1700000000",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");

    let pool = proxy.db().await;
    let listens: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT username, artist, title, listened_at_epoch FROM listens ORDER BY title",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(listens.len(), 2, "{listens:?}");
    assert!(listens.iter().any(|l| l.0 == USER
        && l.1 == "Seed Artist"
        && l.2 == "Seed Song"
        && l.3 == 1_700_000_000));
}

#[tokio::test]
async fn r3_discovery_playlist_appears_and_serves_recommendations() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let ytm = spawn_mock_ytm().await;
    let proxy = spawn_r1_proxy(&upstream, ytm).await;
    let seed_id = insert_virtual_seed(&proxy).await;

    let _ = fetch_json(
        &proxy.url,
        &format!("/rest/scrobble?{}&id={seed_id}", auth_query(Some("json"))),
    )
    .await;

    let body = fetch_json(
        &proxy.url,
        &format!("/rest/getPlaylists?{}", auth_query(Some("json"))),
    )
    .await;
    let playlists = body["subsonic-response"]["playlists"]["playlist"]
        .as_array()
        .unwrap();
    let discovery = playlists
        .iter()
        .find(|p| p["id"] == "songarr_discovery")
        .expect("Songarr Discovery playlist should be injected");
    assert_eq!(discovery["name"], "Songarr Discovery");
    assert!(
        discovery["songCount"].as_i64().unwrap() >= 1,
        "summary should include generated tracks: {body}"
    );

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getPlaylist?{}&id=songarr_discovery",
            auth_query(Some("json"))
        ),
    )
    .await;
    let playlist = &body["subsonic-response"]["playlist"];
    assert_eq!(playlist["name"], "Songarr Discovery");
    let entries = playlist["entry"].as_array().unwrap();
    assert!(
        entries.iter().any(|entry| entry["title"] == "Radio New"
            && entry["id"].as_str().unwrap_or("").starts_with("sgr_")),
        "discovery playlist should contain virtual recommendations: {body}"
    );

    let (status, _, raw) = fetch(
        &proxy.url,
        &format!(
            "/rest/getPlaylist?{}&id=songarr_discovery",
            auth_query(None)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(raw.to_vec()).unwrap();
    assert!(xml.contains(r#"name="Songarr Discovery""#), "{xml}");
    assert!(xml.contains("<entry "), "{xml}");
}

#[tokio::test]
async fn provider_failure_falls_back_to_navidrome_for_real_seed() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let proxy = spawn_r1_proxy(&upstream, "http://127.0.0.1:9".into()).await;

    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getSimilarSongs2?{}&id=real_seed&count=5",
            auth_query(Some("json"))
        ),
    )
    .await;
    let songs = similar2_songs(&body);
    assert_eq!(songs.len(), 1, "{body}");
    assert_eq!(songs[0]["id"], "local_dup");
    assert!(
        songs
            .iter()
            .all(|s| !s["id"].as_str().unwrap_or("").starts_with("sgr_")),
        "fallback must be vanilla Navidrome: {body}"
    );

    // Let any failed connect attempts settle before the serial guard drops.
    tokio::time::sleep(Duration::from_millis(25)).await;
}
