//! Lyrics integration tests: mock Navidrome + mock LRCLIB, no real external
//! services. Verifies the OpenSubsonic endpoint shape for virtual and real ids.

mod common;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::Query;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use common::*;
use reqwest::StatusCode;
use serde_json::json;
use songarr_proxy::vtrack::{self, CatalogTrack};

async fn spawn_mock_navidrome() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/rest/getLyricsBySongId", get(get_lyrics))
        .route("/rest/getLyricsBySongId.view", get(get_lyrics))
        .route("/rest/getSong", get(get_song))
        .route("/rest/getSong.view", get(get_song));
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    base
}

async fn get_lyrics(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let id = params.get("id").map(String::as_str).unwrap_or_default();
    if id == "real_failed" {
        return Json(json!({
            "subsonic-response": {
                "status": "failed",
                "version": "1.16.1",
                "type": "navidrome",
                "serverVersion": "0.62.0-test",
                "openSubsonic": true,
                "error": {"code": 70, "message": "data not found"}
            }
        }));
    }
    let structured = if id == "real_has_lyrics" {
        json!([{
            "displayArtist": "Local Artist",
            "displayTitle": "Local Song",
            "lang": "und",
            "synced": false,
            "line": [{"value": "Already upstream"}]
        }])
    } else {
        json!([])
    };
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "lyricsList": {"structuredLyrics": structured}
        }
    }))
}

async fn get_song(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let id = params.get("id").map(String::as_str).unwrap_or_default();
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "song": {
                "id": id,
                "title": "Mock Song",
                "artist": "Mock Artist",
                "album": "Mock Album",
                "duration": 180
            }
        }
    }))
}

async fn spawn_mock_lrclib() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let hit_counter = hits.clone();
    let router = Router::new()
        .route(
            "/api/get",
            get(move |Query(params): Query<HashMap<String, String>>| {
                let hits = hit_counter.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    if params.get("track_name").map(String::as_str) != Some("Mock Song") {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    Json(json!({
                        "duration": 180.0,
                        "instrumental": false,
                        "plainLyrics": "First line\nSecond line",
                        "syncedLyrics": "[00:10.00] First line\n[00:20.50] Second line"
                    }))
                    .into_response()
                }
            }),
        )
        .route(
            "/api/search",
            get(|| async { Json(json!([])).into_response() }),
        );
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (base, hits)
}

async fn spawn_lyrics_proxy() -> (TestProxy, Arc<AtomicUsize>) {
    let navidrome = spawn_mock_navidrome().await;
    let (lrclib, hits) = spawn_mock_lrclib().await;
    let proxy = spawn_proxy_handle(&navidrome, move |config| {
        config.external_search.enabled = false;
        config.recommendations.enabled = false;
        config.lyrics.enabled = true;
        config.lyrics.lrclib_api_base = lrclib;
    })
    .await;
    (proxy, hits)
}

#[tokio::test]
async fn virtual_track_lyrics_come_from_lrclib_and_are_cached() {
    let (proxy, hits) = spawn_lyrics_proxy().await;
    let pool = proxy.db().await;
    let id = vtrack::upsert(
        &pool,
        &CatalogTrack {
            provider: "deezer",
            provider_track_id: "mock-lyrics".into(),
            artist: "Mock Artist".into(),
            title: "Mock Song".into(),
            album: Some("Mock Album".into()),
            duration_ms: Some(180_000),
            isrc: None,
            artwork_url: None,
        },
    )
    .await
    .unwrap();

    let path = format!(
        "/rest/getLyricsBySongId?{}&id={id}",
        auth_query(Some("json"))
    );
    let body = fetch_json(&proxy.url, &path).await;
    let lyrics = &body["subsonic-response"]["lyricsList"]["structuredLyrics"][0];
    assert_eq!(lyrics["displayArtist"], "Mock Artist");
    assert_eq!(lyrics["displayTitle"], "Mock Song");
    assert_eq!(lyrics["synced"], true);
    assert_eq!(lyrics["line"][0]["start"], 10_000);
    assert_eq!(lyrics["line"][1]["value"], "Second line");

    let body = fetch_json(&proxy.url, &path).await;
    assert_eq!(body["subsonic-response"]["status"], "ok");
    assert_eq!(hits.load(Ordering::SeqCst), 1, "second request used cache");
}

#[tokio::test]
async fn real_track_keeps_upstream_lyrics_without_lrclib_lookup() {
    let (proxy, hits) = spawn_lyrics_proxy().await;
    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getLyricsBySongId?{}&id=real_has_lyrics",
            auth_query(Some("json"))
        ),
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["lyricsList"]["structuredLyrics"][0]["line"][0]["value"],
        "Already upstream"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn real_track_empty_upstream_falls_back_to_lrclib() {
    let (proxy, hits) = spawn_lyrics_proxy().await;
    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getLyricsBySongId?{}&id=real_empty",
            auth_query(Some("json"))
        ),
    )
    .await;
    let lyrics = &body["subsonic-response"]["lyricsList"]["structuredLyrics"][0];
    assert_eq!(lyrics["displayArtist"], "Mock Artist");
    assert_eq!(lyrics["line"][0]["value"], "First line");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn real_track_failed_upstream_falls_back_to_lrclib() {
    let (proxy, hits) = spawn_lyrics_proxy().await;
    let body = fetch_json(
        &proxy.url,
        &format!(
            "/rest/getLyricsBySongId?{}&id=real_failed",
            auth_query(Some("json"))
        ),
    )
    .await;
    let lyrics = &body["subsonic-response"]["lyricsList"]["structuredLyrics"][0];
    assert_eq!(lyrics["displayArtist"], "Mock Artist");
    assert_eq!(lyrics["line"][0]["value"], "First line");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}
