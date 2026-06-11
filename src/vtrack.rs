//! Virtual track ids and persistence.
//!
//! A virtual id is `sgr_` + 22-char base62 of a UUIDv4. Ids are stable per
//! (provider, provider_track_id): repeated searches for the same external
//! track reuse the stored id, so client caches stay coherent.

use sqlx::SqlitePool;
use uuid::Uuid;

pub const ID_PREFIX: &str = "sgr_";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VirtualTrack {
    pub id: String,
    pub provider: String,
    pub provider_track_id: String,
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub isrc: Option<String>,
    pub artwork_url: Option<String>,
    pub status: String,
    pub real_subsonic_id: Option<String>,
    pub resolved_url: Option<String>,
    pub resolved_score: Option<i64>,
    pub resolved_title: Option<String>,
    pub resolved_at_epoch: Option<i64>,
}

pub fn is_virtual_id(id: &str) -> bool {
    id.starts_with(ID_PREFIX)
}

pub fn new_virtual_id() -> String {
    format!("{ID_PREFIX}{}", base62_22(Uuid::new_v4().as_u128()))
}

/// Fixed-width 22-char base62 (62^22 > 2^128, zero-padded).
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

/// Metadata from a catalog provider, ready to become a virtual track.
#[derive(Debug, Clone)]
pub struct CatalogTrack {
    pub provider: &'static str,
    pub provider_track_id: String,
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub isrc: Option<String>,
    pub artwork_url: Option<String>,
}

/// Insert or refresh a virtual track, returning its stable `sgr_` id.
pub async fn upsert(pool: &SqlitePool, track: &CatalogTrack) -> sqlx::Result<String> {
    let candidate_id = new_virtual_id();
    let now = now_utc();
    // On conflict the existing row (and id) wins; metadata is refreshed.
    let id: String = sqlx::query_scalar(
        r#"
        INSERT INTO virtual_tracks
            (id, provider, provider_track_id, artist, title, album,
             duration_ms, isrc, artwork_url, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(provider, provider_track_id) DO UPDATE SET
            artist = excluded.artist,
            title = excluded.title,
            album = excluded.album,
            duration_ms = excluded.duration_ms,
            isrc = COALESCE(excluded.isrc, virtual_tracks.isrc),
            artwork_url = COALESCE(excluded.artwork_url, virtual_tracks.artwork_url),
            updated_at = excluded.updated_at
        RETURNING id
        "#,
    )
    .bind(&candidate_id)
    .bind(track.provider)
    .bind(&track.provider_track_id)
    .bind(&track.artist)
    .bind(&track.title)
    .bind(&track.album)
    .bind(track.duration_ms)
    .bind(&track.isrc)
    .bind(&track.artwork_url)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

const COLUMNS: &str = "id, provider, provider_track_id, artist, title, album, duration_ms,
    isrc, artwork_url, status, real_subsonic_id,
    resolved_url, resolved_score, resolved_title, resolved_at_epoch";

pub async fn get(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<VirtualTrack>> {
    sqlx::query_as::<_, VirtualTrack>(&format!(
        "SELECT {COLUMNS} FROM virtual_tracks WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Imported tracks, for search dedup (their real files are in the library).
pub async fn imported(pool: &SqlitePool) -> sqlx::Result<Vec<VirtualTrack>> {
    sqlx::query_as::<_, VirtualTrack>(&format!(
        "SELECT {COLUMNS} FROM virtual_tracks WHERE status = 'imported'"
    ))
    .fetch_all(pool)
    .await
}

/// Persist a yt-dlp resolution so future plays skip the search round-trip.
pub async fn set_resolution(
    pool: &SqlitePool,
    id: &str,
    url: &str,
    score: i64,
    candidate_title: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE virtual_tracks SET resolved_url = ?, resolved_score = ?, resolved_title = ?,
         resolved_at_epoch = ?, updated_at = ? WHERE id = ?",
    )
    .bind(url)
    .bind(score)
    .bind(candidate_title)
    .bind(epoch_secs())
    .bind(now_utc())
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
}

pub fn epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub async fn mark_imported(pool: &SqlitePool, id: &str, real_id: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE virtual_tracks SET real_subsonic_id = ?, status = 'imported', fail_reason = NULL,
         updated_at = ? WHERE id = ?",
    )
    .bind(real_id)
    .bind(now_utc())
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn set_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    fail_reason: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE virtual_tracks SET status = ?, fail_reason = ?, updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(fail_reason)
    .bind(now_utc())
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
}

pub fn now_utc() -> String {
    // ISO-8601 UTC without subsecond noise; std-only (no chrono needed yet).
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = secs / 86_400;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_ids_have_expected_shape() {
        let id = new_virtual_id();
        assert_eq!(id.len(), ID_PREFIX.len() + 22);
        assert!(is_virtual_id(&id));
        assert!(id[ID_PREFIX.len()..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(id, new_virtual_id());
    }

    #[test]
    fn base62_is_fixed_width_and_ordered() {
        assert_eq!(base62_22(0).len(), 22);
        assert_eq!(base62_22(u128::MAX).len(), 22);
        assert_eq!(base62_22(0), "0".repeat(22));
        assert_eq!(base62_22(61), format!("{}z", "0".repeat(21)));
        assert_eq!(base62_22(62), format!("{}10", "0".repeat(20)));
    }

    #[test]
    fn timestamps_look_iso8601() {
        let ts = now_utc();
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.starts_with("20") && ts.ends_with('Z'), "{ts}");
    }

    fn track(n: u32) -> CatalogTrack {
        CatalogTrack {
            provider: "deezer",
            provider_track_id: n.to_string(),
            artist: "Artist".into(),
            title: format!("Title {n}"),
            album: Some("Album".into()),
            duration_ms: Some(200_000),
            isrc: None,
            artwork_url: Some("https://example.com/a.jpg".into()),
        }
    }

    #[tokio::test]
    async fn upsert_is_stable_per_provider_track() {
        let pool = crate::db::init(
            &std::env::temp_dir()
                .join(format!("songarr-vt-{}", Uuid::new_v4()))
                .join("t.db"),
        )
        .await
        .unwrap();

        let first = upsert(&pool, &track(1)).await.unwrap();
        let again = upsert(&pool, &track(1)).await.unwrap();
        let other = upsert(&pool, &track(2)).await.unwrap();
        assert_eq!(first, again, "same provider track must keep its id");
        assert_ne!(first, other);

        let loaded = get(&pool, &first).await.unwrap().unwrap();
        assert_eq!(loaded.title, "Title 1");
        assert_eq!(loaded.status, "virtual");
        assert!(get(&pool, "sgr_missing").await.unwrap().is_none());
    }
}
