//! Virtual album ids and persistence for artist expansion.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::vtrack::VirtualTrack;

pub const ID_PREFIX: &str = "sga_";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VirtualAlbum {
    pub id: String,
    pub provider: String,
    pub provider_album_id: String,
    pub artist: String,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub artwork_url: Option<String>,
    pub track_count: Option<i64>,
    pub payload_json: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct NewVirtualAlbum {
    pub provider: &'static str,
    pub provider_album_id: String,
    pub artist: String,
    pub title: String,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    pub artwork_url: Option<String>,
    pub track_count: Option<i64>,
    pub payload: AlbumPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumPayload {
    #[serde(default)]
    pub tracks: Vec<AlbumTrackPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumTrackPayload {
    pub provider_track_id: String,
    pub artist: String,
    pub title: String,
    pub album: String,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub disc_number: Option<i64>,
    #[serde(default)]
    pub track_number: Option<i64>,
}

pub fn is_virtual_album_id(id: &str) -> bool {
    id.starts_with(ID_PREFIX)
}

pub fn new_virtual_album_id() -> String {
    format!("{ID_PREFIX}{}", base62_22(Uuid::new_v4().as_u128()))
}

fn base62_22(mut n: u128) -> String {
    const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut out = [b'0'; 22];
    for slot in out.iter_mut().rev() {
        *slot = ALPHABET[(n % 62) as usize];
        n /= 62;
        if n == 0 {
            break;
        }
    }
    String::from_utf8(out.to_vec()).unwrap()
}

pub async fn upsert(pool: &SqlitePool, album: &NewVirtualAlbum) -> sqlx::Result<String> {
    let candidate_id = new_virtual_album_id();
    let payload_json =
        serde_json::to_string(&album.payload).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let now = crate::vtrack::now_utc();
    let id: String = sqlx::query_scalar(
        r#"
        INSERT INTO virtual_albums
            (id, provider, provider_album_id, artist, title, album_type,
             release_date, artwork_url, track_count, payload_json, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(provider, provider_album_id) DO UPDATE SET
            artist = excluded.artist,
            title = excluded.title,
            album_type = excluded.album_type,
            release_date = excluded.release_date,
            artwork_url = COALESCE(excluded.artwork_url, virtual_albums.artwork_url),
            track_count = excluded.track_count,
            payload_json = excluded.payload_json,
            updated_at = excluded.updated_at
        RETURNING id
        "#,
    )
    .bind(&candidate_id)
    .bind(album.provider)
    .bind(&album.provider_album_id)
    .bind(&album.artist)
    .bind(&album.title)
    .bind(&album.album_type)
    .bind(&album.release_date)
    .bind(&album.artwork_url)
    .bind(album.track_count)
    .bind(&payload_json)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

const COLUMNS: &str = "id, provider, provider_album_id, artist, title, album_type,
    release_date, artwork_url, track_count, payload_json, status";

pub async fn get(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<VirtualAlbum>> {
    sqlx::query_as::<_, VirtualAlbum>(&format!(
        "SELECT {COLUMNS} FROM virtual_albums WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn cache_get(
    pool: &SqlitePool,
    provider: &str,
    artist_key: &str,
    ttl_hours: u32,
) -> sqlx::Result<Option<String>> {
    let Some((payload, fetched_at)): Option<(String, i64)> = sqlx::query_as(
        "SELECT payload_json, fetched_at_epoch FROM artist_catalog_cache
         WHERE provider = ? AND artist_key = ?",
    )
    .bind(provider)
    .bind(artist_key)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let age = crate::vtrack::epoch_secs().saturating_sub(fetched_at);
    if age <= i64::from(ttl_hours) * 3600 {
        Ok(Some(payload))
    } else {
        Ok(None)
    }
}

pub async fn cache_set(
    pool: &SqlitePool,
    provider: &str,
    artist_key: &str,
    payload_json: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO artist_catalog_cache (provider, artist_key, payload_json, fetched_at_epoch)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(provider, artist_key) DO UPDATE SET
            payload_json = excluded.payload_json,
            fetched_at_epoch = excluded.fetched_at_epoch
        "#,
    )
    .bind(provider)
    .bind(artist_key)
    .bind(payload_json)
    .bind(crate::vtrack::epoch_secs())
    .execute(pool)
    .await
    .map(|_| ())
}

/// Fill a virtual track's missing artwork from any cached virtual album
/// payload that contains the same provider track id or same song identity.
pub async fn repair_track_artwork(pool: &SqlitePool, track: &mut VirtualTrack) -> sqlx::Result<()> {
    if track.artwork_url.is_some() {
        return Ok(());
    }
    let Some(artwork_url) = find_artwork_for_track(pool, track).await? else {
        return Ok(());
    };
    crate::vtrack::set_artwork_if_missing(pool, &track.id, &artwork_url).await?;
    track.artwork_url = Some(artwork_url);
    Ok(())
}

pub async fn find_artwork_for_track(
    pool: &SqlitePool,
    track: &VirtualTrack,
) -> sqlx::Result<Option<String>> {
    let rows: Vec<(Option<String>, String)> =
        sqlx::query_as("SELECT artwork_url, payload_json FROM virtual_albums WHERE provider = ?")
            .bind(&track.provider)
            .fetch_all(pool)
            .await?;
    let wanted_key = crate::recs::song_key(&track.artist, &track.title);
    for (album_artwork, payload_json) in rows {
        let Ok(payload) = serde_json::from_str::<AlbumPayload>(&payload_json) else {
            continue;
        };
        for payload_track in payload.tracks {
            let exact_provider_id = payload_track.provider_track_id == track.provider_track_id;
            let same_song =
                crate::recs::song_key(&payload_track.artist, &payload_track.title) == wanted_key;
            if exact_provider_id || same_song {
                return Ok(payload_track.artwork_url.or_else(|| album_artwork.clone()));
            }
        }
    }
    Ok(None)
}

pub fn album_artwork_url(album: &VirtualAlbum) -> Option<String> {
    album.artwork_url.clone().or_else(|| {
        serde_json::from_str::<AlbumPayload>(&album.payload_json)
            .ok()
            .and_then(|payload| {
                payload
                    .tracks
                    .into_iter()
                    .find_map(|track| track.artwork_url)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_album_ids_have_expected_shape() {
        let id = new_virtual_album_id();
        assert_eq!(id.len(), ID_PREFIX.len() + 22);
        assert!(is_virtual_album_id(&id));
    }

    #[tokio::test]
    async fn repairs_track_artwork_from_album_payload() {
        let pool = crate::db::init(
            &std::env::temp_dir()
                .join(format!("songarr-valbum-art-{}", uuid::Uuid::new_v4()))
                .join("t.db"),
        )
        .await
        .unwrap();
        let album = NewVirtualAlbum {
            provider: crate::catalog::deezer::PROVIDER,
            provider_album_id: "album1".into(),
            artist: "Molchat Doma".into(),
            title: "Этажи".into(),
            album_type: Some("album".into()),
            release_date: None,
            artwork_url: Some("https://example.com/album.jpg".into()),
            track_count: Some(1),
            payload: AlbumPayload {
                tracks: vec![AlbumTrackPayload {
                    provider_track_id: "track1".into(),
                    artist: "Молчат Дома".into(),
                    title: "Судно (Борис Рыжий)".into(),
                    album: "Этажи".into(),
                    duration_ms: Some(130_000),
                    isrc: None,
                    artwork_url: None,
                    disc_number: Some(1),
                    track_number: Some(1),
                }],
            },
        };
        upsert(&pool, &album).await.unwrap();
        let track_id = crate::vtrack::upsert(
            &pool,
            &crate::vtrack::CatalogTrack {
                provider: crate::catalog::deezer::PROVIDER,
                provider_track_id: "other-id".into(),
                artist: "Molchat Doma".into(),
                title: "Sudno".into(),
                album: None,
                duration_ms: Some(130_000),
                isrc: None,
                artwork_url: None,
            },
        )
        .await
        .unwrap();
        let mut track = crate::vtrack::get(&pool, &track_id).await.unwrap().unwrap();
        repair_track_artwork(&pool, &mut track).await.unwrap();
        assert_eq!(
            track.artwork_url.as_deref(),
            Some("https://example.com/album.jpg")
        );
    }
}
