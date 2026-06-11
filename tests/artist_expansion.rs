//! Artist expansion integration tests: mock Navidrome + mock Deezer, no real
//! provider traffic.

mod common;

use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use common::*;
use serde_json::json;
use songarr_proxy::vtrack::{self, CatalogTrack};

async fn spawn_artist_proxy(upstream: &str, deezer: String) -> TestProxy {
    spawn_proxy_handle(upstream, move |config| {
        config.external_search.enabled = false;
        config.external_search.api_base_deezer = deezer;
        config.artist_expansion.enabled = true;
        config.artist_expansion.max_albums = 6;
        config.artist_expansion.max_tracks_per_album = 10;
        config.artist_expansion.include_top_tracks_album = false;
    })
    .await
}

async fn spawn_mock_navidrome() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/rest/ping", get(ping))
        .route("/rest/getSong", get(get_song))
        .route("/rest/getSong.view", get(get_song))
        .route("/rest/stream", get(stream_real))
        .route("/rest/stream.view", get(stream_real))
        .route("/rest/getArtist", get(get_artist))
        .route("/rest/getArtist.view", get(get_artist))
        .route("/rest/getAlbum", get(get_album_passthrough))
        .route("/rest/getAlbum.view", get(get_album_passthrough));
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

async fn get_song(Query(params): Query<std::collections::HashMap<String, String>>) -> Response {
    let id = params.get("id").map(String::as_str).unwrap_or("");
    Json(json!({
        "subsonic-response": {
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "song": {
                "id": id,
                "title": "Первый трек",
                "artist": "Оксимирон",
                "duration": 180
            }
        }
    }))
    .into_response()
}

async fn stream_real() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "audio/mpeg")],
        Body::from(vec![1_u8, 2, 3, 4]),
    )
        .into_response()
}

async fn get_artist(Query(params): Query<std::collections::HashMap<String, String>>) -> Response {
    let format = params.get("f").map(String::as_str).unwrap_or("xml");
    if format == "json" {
        Json(json!({
            "subsonic-response": {
                "status": "ok",
                "version": "1.16.1",
                "type": "navidrome",
                "serverVersion": "0.62.0-test",
                "openSubsonic": true,
                "artist": {
                    "id": "artist_oxx",
                    "name": "Оксимирон",
                    "albumCount": 1,
                    "album": [{
                        "id": "local_gorgorod",
                        "name": "Горгород",
                        "artist": "Оксимирон",
                        "artistId": "artist_oxx",
                        "songCount": 1,
                        "duration": 200,
                        "created": "2025-01-01T00:00:00Z"
                    }]
                }
            }
        }))
        .into_response()
    } else {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><subsonic-response xmlns="http://subsonic.org/restapi" status="ok" version="1.16.1" type="navidrome" serverVersion="0.62.0-test" openSubsonic="true"><artist id="artist_oxx" name="Оксимирон" albumCount="1"><album id="local_gorgorod" name="Горгород" artist="Оксимирон" artistId="artist_oxx" songCount="1" duration="200" created="2025-01-01T00:00:00Z"/></artist></subsonic-response>"#;
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/xml")],
            Body::from(body),
        )
            .into_response()
    }
}

async fn get_album_passthrough() -> Json<serde_json::Value> {
    Json(json!({
        "subsonic-response": {
            "status": "failed",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.62.0-test",
            "openSubsonic": true,
            "error": {"code": 70, "message": "data not found"}
        }
    }))
}

async fn spawn_mock_deezer_artist() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let art_base = base.clone();
    let router = Router::new()
        .route("/search/artist", get(search_artist))
        .route("/artist/{id}/albums", get(artist_albums))
        .route("/album/{id}", get(album_detail))
        .route(
            "/art/{file}",
            get(move |Path(_file): Path<String>| async move {
                ([("content-type", "image/jpeg")], FAKE_JPEG.to_vec())
            }),
        )
        .with_state(art_base);
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    base
}

async fn search_artist() -> Json<serde_json::Value> {
    Json(json!({
        "data": [
            {"id": 99, "name": "Oxxxy Miron Tribute", "nb_fan": 999999},
            {"id": 100, "name": "Oxxxymiron", "nb_fan": 5000, "picture_big": "http://unused/art/artist.jpg"}
        ],
        "total": 2
    }))
}

async fn artist_albums(
    axum::extract::State(base): axum::extract::State<String>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    assert_eq!(id, 100);
    Json(json!({
        "data": [
            {"id": 200, "title": "Горгород", "record_type": "album", "release_date": "2015-11-13", "nb_tracks": 1, "cover_big": format!("{base}/art/200-summary.jpg")},
            {"id": 201, "title": "Красота и Уродство", "record_type": "album", "release_date": "2021-12-01", "nb_tracks": 2, "cover_big": format!("{base}/art/201-summary.jpg")}
        ],
        "total": 2
    }))
}

async fn album_detail(
    axum::extract::State(base): axum::extract::State<String>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    let (title, tracks) = match id {
        200 => (
            "Горгород",
            json!([{
                "id": 300,
                "title": "Переплетено",
                "duration": 200,
                "artist": {"name": "Oxxxymiron"},
                "disk_number": 1,
                "track_position": 1
            }]),
        ),
        201 => (
            "Красота и Уродство",
            json!([
                {
                    "id": 301,
                    "title": "Кто убил Марка?",
                    "duration": 185,
                    "artist": {"name": "Oxxxymiron"},
                    "disk_number": 1,
                    "track_position": 1
                },
                {
                    "id": 302,
                    "title": "Организация",
                    "duration": 214,
                    "artist": {"name": "Oxxxymiron"},
                    "disk_number": 1,
                    "track_position": 2
                }
            ]),
        ),
        _ => panic!("unexpected album id {id}"),
    };
    Json(json!({
        "id": id,
        "title": title,
        "record_type": "album",
        "release_date": if id == 201 { "2021-12-01" } else { "2015-11-13" },
        "cover_big": if id == 200 { json!(format!("{base}/art/{id}.jpg")) } else { serde_json::Value::Null },
        "nb_tracks": tracks.as_array().unwrap().len(),
        "artist": {"id": 100, "name": "Oxxxymiron"},
        "tracks": {"data": tracks}
    }))
}

#[tokio::test]
async fn get_artist_appends_deduped_virtual_album_json_and_xml() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let deezer = spawn_mock_deezer_artist().await;
    let proxy = spawn_artist_proxy(&upstream, deezer).await;
    let auth = auth_query(Some("json"));

    let body = fetch_json(&proxy.url, &format!("/rest/getArtist?{auth}&id=artist_oxx")).await;
    let albums = body["subsonic-response"]["artist"]["album"]
        .as_array()
        .unwrap();
    assert_eq!(
        albums.len(),
        2,
        "local Горгород plus one missing album: {body}"
    );
    assert_eq!(albums[0]["id"], "local_gorgorod");
    assert_eq!(albums[1]["name"], "Красота и Уродство");
    let virtual_album_id = albums[1]["id"].as_str().unwrap();
    assert!(virtual_album_id.starts_with("sga_"));

    let body2 = fetch_json(&proxy.url, &format!("/rest/getArtist?{auth}&id=artist_oxx")).await;
    assert_eq!(
        body2["subsonic-response"]["artist"]["album"][1]["id"], virtual_album_id,
        "virtual album id should be stable across cached expansions"
    );

    let xml_auth = auth_query(None);
    let (_status, _headers, xml_body) = fetch(
        &proxy.url,
        &format!("/rest/getArtist?{xml_auth}&id=artist_oxx"),
    )
    .await;
    let xml = String::from_utf8(xml_body.to_vec()).unwrap();
    assert!(xml.contains(virtual_album_id), "{xml}");
    assert!(!xml.contains(r#"id="sga_" name="Горгород""#), "{xml}");
}

#[tokio::test]
async fn virtual_album_returns_playable_virtual_tracks_and_cover_art() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let deezer = spawn_mock_deezer_artist().await;
    let proxy = spawn_artist_proxy(&upstream, deezer).await;
    let auth = auth_query(Some("json"));

    let artist = fetch_json(&proxy.url, &format!("/rest/getArtist?{auth}&id=artist_oxx")).await;
    let album_id = artist["subsonic-response"]["artist"]["album"][1]["id"]
        .as_str()
        .unwrap();
    let album = fetch_json(&proxy.url, &format!("/rest/getAlbum?{auth}&id={album_id}")).await;
    let album_obj = &album["subsonic-response"]["album"];
    assert_eq!(album_obj["id"], album_id);
    assert_eq!(album_obj["name"], "Красота и Уродство");
    assert_eq!(album_obj["coverArt"], album_id);
    let songs = album_obj["song"].as_array().unwrap();
    assert_eq!(songs.len(), 2, "{album}");
    assert!(songs[0]["id"].as_str().unwrap().starts_with("sgr_"));
    assert_eq!(songs[0]["albumId"], album_id);
    assert_eq!(songs[0]["track"], 1);

    let (status, headers, art) = fetch(
        &proxy.url,
        &format!("/rest/getCoverArt?{auth}&id={album_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(art.as_ref(), FAKE_JPEG);

    let track_id = songs[0]["id"].as_str().unwrap();
    let (status, headers, bytes) =
        fetch(&proxy.url, &format!("/rest/stream?{auth}&id={track_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .contains("audio/ogg"));
    assert!(!bytes.is_empty(), "stream should return transcoded bytes");
}

#[tokio::test]
async fn provider_failure_returns_vanilla_artist_response() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let deezer = format!("{}/missing", spawn_mock_deezer_artist().await);
    let proxy = spawn_artist_proxy(&upstream, deezer).await;
    let auth = auth_query(Some("json"));

    let body = fetch_json(&proxy.url, &format!("/rest/getArtist?{auth}&id=artist_oxx")).await;
    let albums = body["subsonic-response"]["artist"]["album"]
        .as_array()
        .unwrap();
    assert_eq!(
        albums.len(),
        1,
        "provider errors should not break getArtist"
    );
    assert_eq!(albums[0]["id"], "local_gorgorod");
}

#[tokio::test]
async fn unknown_virtual_album_returns_subsonic_not_found() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let deezer = spawn_mock_deezer_artist().await;
    let proxy = spawn_artist_proxy(&upstream, deezer).await;
    let auth = auth_query(Some("json"));

    let body = fetch_json(
        &proxy.url,
        &format!("/rest/getAlbum?{auth}&id=sga_0000000000000000000000"),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert_eq!(body["subsonic-response"]["error"]["code"], 70);
}

#[tokio::test]
async fn scrobbling_first_virtual_song_prewarms_artist_catalog_and_artwork() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let deezer = spawn_mock_deezer_artist().await;
    let proxy = spawn_artist_proxy(&upstream, deezer).await;
    let pool = proxy.db().await;
    let seed_id = vtrack::upsert(
        &pool,
        &CatalogTrack {
            provider: songarr_proxy::catalog::deezer::PROVIDER,
            provider_track_id: "seed-sudno".into(),
            artist: "Оксимирон".into(),
            title: "Первый трек".into(),
            album: None,
            duration_ms: Some(180_000),
            isrc: None,
            artwork_url: None,
        },
    )
    .await
    .unwrap();

    let auth = auth_query(Some("json"));
    let body = fetch_json(
        &proxy.url,
        &format!("/rest/scrobble?{auth}&id={seed_id}&time=1700000000"),
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");

    let mut album_ids = Vec::new();
    for _ in 0..50 {
        album_ids = sqlx::query_scalar::<_, String>("SELECT id FROM virtual_albums ORDER BY title")
            .fetch_all(&pool)
            .await
            .unwrap();
        if album_ids.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        album_ids.len(),
        2,
        "prewarm should store both provider albums without opening getArtist"
    );

    let materialized: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM virtual_tracks WHERE provider_track_id IN ('300', '301', '302')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(materialized, 3, "prewarm should materialize album tracks");

    let art_path = proxy
        .db_path
        .parent()
        .unwrap()
        .join("artwork")
        .join(format!("{}.img", album_ids[0]));
    let cached = tokio::fs::read(&art_path)
        .await
        .unwrap_or_else(|e| panic!("expected cached artwork at {}: {e}", art_path.display()));
    assert_eq!(cached, FAKE_JPEG);
}

#[tokio::test]
async fn streaming_real_song_prewarms_artist_without_waiting_for_scrobble() {
    let _guard = serial();
    let upstream = spawn_mock_navidrome().await;
    let deezer = spawn_mock_deezer_artist().await;
    let proxy = spawn_artist_proxy(&upstream, deezer).await;
    let pool = proxy.db().await;
    let auth = auth_query(Some("json"));

    let (status, _headers, bytes) = fetch(
        &proxy.url,
        &format!("/rest/stream?{auth}&id=real_local_song"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes.as_ref(), &[1_u8, 2, 3, 4]);

    let mut album_ids = Vec::new();
    for _ in 0..50 {
        album_ids = sqlx::query_scalar::<_, String>("SELECT id FROM virtual_albums ORDER BY title")
            .fetch_all(&pool)
            .await
            .unwrap();
        if album_ids.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        album_ids.len(),
        2,
        "real stream should prewarm artist catalog even before any scrobble"
    );

    let mut dir = tokio::fs::read_dir(proxy.db_path.parent().unwrap().join("artwork"))
        .await
        .unwrap();
    let mut cached_art_count = 0;
    while dir.next_entry().await.unwrap().is_some() {
        cached_art_count += 1;
    }
    assert!(
        cached_art_count >= 2,
        "prewarm should cache album artwork, including summary-art fallback"
    );
}
