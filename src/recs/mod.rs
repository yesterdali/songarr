//! Recommendation providers (songarr-recs-plan.md). Each provider turns a
//! seed into `Vec<RecCandidate>` — normalized (artist, title) pairs that the
//! similar/topSongs handlers upsert as virtual tracks.
//!
//! Failure doctrine (same as streaming): a provider that errors contributes
//! nothing — recommendations degrade in quality, never break the endpoint.

pub mod deezer;
pub mod lastfm;
pub mod merge;
pub mod ytm;

use sqlx::SqlitePool;

/// One recommended track, as uniform as providers can make it.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct RecCandidate {
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    /// Stable provider identity when a source has one (`ytmusic` video id,
    /// Deezer track id). Missing candidates are keyed by normalized
    /// artist/title when persisted.
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub provider_track_id: Option<String>,
    /// Set when the provider already knows the exact YouTube video — stored
    /// as the resolution at upsert time so pressing play skips the whole
    /// yt-dlp search round-trip.
    #[serde(default)]
    pub video_id: Option<String>,
}

impl RecCandidate {
    pub fn song_key(&self) -> String {
        song_key(&self.artist, &self.title)
    }
}

pub fn song_key(artist: &str, title: &str) -> String {
    format!("{}|{}", normalize(artist), normalize(title))
}

pub fn normalize(value: &str) -> String {
    deunicode::deunicode(value)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

pub async fn cache_get(
    pool: &SqlitePool,
    source: &str,
    seed_key: &str,
    ttl_hours: u32,
) -> sqlx::Result<Option<Vec<RecCandidate>>> {
    if ttl_hours == 0 {
        return Ok(None);
    }
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT payload_json, fetched_at_epoch FROM rec_cache WHERE source = ? AND seed_key = ?",
    )
    .bind(source)
    .bind(seed_key)
    .fetch_optional(pool)
    .await?;
    let Some((payload, fetched_at)) = row else {
        return Ok(None);
    };
    let age = crate::vtrack::epoch_secs() - fetched_at;
    if age > i64::from(ttl_hours) * 3600 {
        return Ok(None);
    }
    Ok(serde_json::from_str(&payload).ok())
}

pub async fn cache_set(
    pool: &SqlitePool,
    source: &str,
    seed_key: &str,
    candidates: &[RecCandidate],
) -> sqlx::Result<()> {
    let payload = serde_json::to_string(candidates).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "INSERT INTO rec_cache (source, seed_key, payload_json, fetched_at_epoch)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(source, seed_key) DO UPDATE SET
            payload_json = excluded.payload_json,
            fetched_at_epoch = excluded.fetched_at_epoch",
    )
    .bind(source)
    .bind(seed_key)
    .bind(payload)
    .bind(crate::vtrack::epoch_secs())
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn recently_shown_keys(
    pool: &SqlitePool,
    username: &str,
    cooldown_days: u32,
) -> sqlx::Result<std::collections::HashSet<String>> {
    if username.is_empty() || cooldown_days == 0 {
        return Ok(std::collections::HashSet::new());
    }
    let cutoff = crate::vtrack::epoch_secs() - i64::from(cooldown_days) * 86_400;
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT song_key FROM rec_shown WHERE username = ? AND shown_at_epoch >= ?")
            .bind(username)
            .bind(cutoff)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(key,)| key).collect())
}

pub async fn mark_shown(
    pool: &SqlitePool,
    username: &str,
    candidates: &[RecCandidate],
) -> sqlx::Result<()> {
    if username.is_empty() || candidates.is_empty() {
        return Ok(());
    }
    let now = crate::vtrack::epoch_secs();
    for candidate in candidates {
        sqlx::query(
            "INSERT INTO rec_shown (username, song_key, shown_at_epoch)
             VALUES (?, ?, ?)
             ON CONFLICT(username, song_key) DO UPDATE SET
                shown_at_epoch = excluded.shown_at_epoch",
        )
        .bind(username)
        .bind(candidate.song_key())
        .bind(now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ListenSeed {
    pub username: String,
    pub artist: String,
    pub title: String,
    pub subsonic_id: Option<String>,
}

pub async fn record_listen(
    pool: &SqlitePool,
    username: &str,
    artist: &str,
    title: &str,
    subsonic_id: Option<&str>,
    listened_at_epoch: i64,
) -> sqlx::Result<()> {
    if username.is_empty() || artist.trim().is_empty() || title.trim().is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO listens (id, username, artist, title, subsonic_id, listened_at_epoch)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(username)
    .bind(artist)
    .bind(title)
    .bind(subsonic_id)
    .bind(listened_at_epoch)
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn recent_listen_seeds(
    pool: &SqlitePool,
    username: &str,
    limit: usize,
) -> sqlx::Result<Vec<ListenSeed>> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT artist, title, subsonic_id
         FROM listens
         WHERE username = ?
         ORDER BY listened_at_epoch DESC
         LIMIT 100",
    )
    .bind(username)
    .fetch_all(pool)
    .await?;

    let mut seen = std::collections::HashSet::new();
    let mut seeds = Vec::new();
    for (artist, title, subsonic_id) in rows {
        let key = song_key(&artist, &title);
        if seen.insert(key) {
            seeds.push(ListenSeed {
                username: username.to_string(),
                artist,
                title,
                subsonic_id,
            });
            if seeds.len() >= limit {
                break;
            }
        }
    }
    Ok(seeds)
}

pub async fn discovery_cache_get(
    pool: &SqlitePool,
    username: &str,
) -> sqlx::Result<Option<Vec<RecCandidate>>> {
    cache_get(pool, "discovery", username, 6 * 24).await
}

pub async fn discovery_cache_set(
    pool: &SqlitePool,
    username: &str,
    candidates: &[RecCandidate],
) -> sqlx::Result<()> {
    cache_set(pool, "discovery", username, candidates).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_key_normalizes_cyrillic_and_punctuation() {
        assert_eq!(
            song_key("Скриптонит", "Где твоя любовь?"),
            song_key("Skriptonit", "Gde tvoia liubov")
        );
        assert_eq!(song_key("AC/DC", "T.N.T."), "acdc|tnt");
    }

    #[tokio::test]
    async fn recent_listen_seeds_are_distinct_newest_first() {
        let pool = crate::db::init(
            &std::env::temp_dir()
                .join(format!("songarr-listens-{}", uuid::Uuid::new_v4()))
                .join("t.db"),
        )
        .await
        .unwrap();
        record_listen(&pool, "u", "A", "First", Some("1"), 10)
            .await
            .unwrap();
        record_listen(&pool, "u", "B", "Second", Some("2"), 20)
            .await
            .unwrap();
        record_listen(&pool, "u", "A", "First", Some("1"), 30)
            .await
            .unwrap();
        let seeds = recent_listen_seeds(&pool, "u", 10).await.unwrap();
        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[0].title, "First");
        assert_eq!(seeds[1].title, "Second");
    }
}
