//! Lyrics lookup: cache → LRCLIB. Keyed by song identity (`recs::song_key`)
//! so every audio source for the same song shares one cache row. Built as a
//! provider chain so VK (strong on the Russian catalog) can slot in later.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppState;

/// How long a "no lyrics found" answer is trusted before re-asking.
const NEGATIVE_TTL_SECS: i64 = 7 * 86_400;
/// Synced timestamps only make sense if the provider's recording length is
/// close to ours; beyond this we keep the plain text and drop the timing.
const SYNC_DURATION_TOLERANCE_SECS: i64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncedLine {
    pub start_ms: i64,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub artist: String,
    pub title: String,
    pub plain: Option<String>,
    pub synced: Option<Vec<SyncedLine>>,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.plain.is_none() && self.synced.is_none()
    }
}

/// Look up lyrics for a song, hitting the cache first. `Ok(None)` means
/// "looked, found nothing" (also cached).
pub async fn lookup(
    state: &AppState,
    artist: &str,
    title: &str,
    album: Option<&str>,
    duration_secs: Option<i64>,
) -> anyhow::Result<Option<Lyrics>> {
    if !state.config.lyrics.enabled {
        return Ok(None);
    }

    let song_key = crate::recs::song_key(artist, title);
    let now = crate::vtrack::epoch_secs();

    if let Some(cached) = cache_get(&state.db, &song_key).await? {
        let stale_miss =
            cached.provider == "none" && now - cached.fetched_at_epoch > NEGATIVE_TTL_SECS;
        let duration_mismatch = match (cached.duration_secs, duration_secs) {
            (Some(cached), Some(current)) => {
                (cached - current).abs() > SYNC_DURATION_TOLERANCE_SECS
            }
            _ => false,
        };
        if !stale_miss && !duration_mismatch {
            return Ok(cached.into_lyrics(artist, title));
        }
    }

    let fetched = fetch_lrclib(state, artist, title, album, duration_secs).await;
    let fetched = match fetched {
        Ok(fetched) => fetched,
        Err(error) => {
            // Network trouble: don't poison the cache, just report no lyrics.
            tracing::debug!(%error, artist, title, "lrclib lookup failed");
            return Ok(None);
        }
    };

    let row = CacheRow {
        provider: if fetched.is_some() { "lrclib" } else { "none" }.into(),
        plain: fetched.as_ref().and_then(|l| l.plain.clone()),
        synced_json: fetched
            .as_ref()
            .and_then(|l| l.synced.as_ref())
            .map(|lines| serde_json::to_string(lines).unwrap_or_default()),
        duration_secs,
        fetched_at_epoch: now,
    };
    cache_put(&state.db, &song_key, artist, title, &row).await?;
    Ok(fetched)
}

struct CacheRow {
    provider: String,
    plain: Option<String>,
    synced_json: Option<String>,
    duration_secs: Option<i64>,
    fetched_at_epoch: i64,
}

impl CacheRow {
    fn into_lyrics(self, artist: &str, title: &str) -> Option<Lyrics> {
        if self.provider == "none" {
            return None;
        }
        let synced = self
            .synced_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<Vec<SyncedLine>>(json).ok())
            .filter(|lines| !lines.is_empty());
        let lyrics = Lyrics {
            artist: artist.to_string(),
            title: title.to_string(),
            plain: self.plain.filter(|text| !text.trim().is_empty()),
            synced,
        };
        if lyrics.is_empty() {
            None
        } else {
            Some(lyrics)
        }
    }
}

async fn cache_get(pool: &SqlitePool, song_key: &str) -> sqlx::Result<Option<CacheRow>> {
    let row: Option<(String, Option<String>, Option<String>, Option<i64>, i64)> = sqlx::query_as(
        "SELECT provider, plain, synced_json, duration_secs, fetched_at_epoch
         FROM lyrics_cache WHERE song_key = ?",
    )
    .bind(song_key)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(provider, plain, synced_json, duration_secs, fetched_at_epoch)| CacheRow {
            provider,
            plain,
            synced_json,
            duration_secs,
            fetched_at_epoch,
        },
    ))
}

async fn cache_put(
    pool: &SqlitePool,
    song_key: &str,
    artist: &str,
    title: &str,
    row: &CacheRow,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO lyrics_cache
            (song_key, artist, title, duration_secs, provider, plain, synced_json, fetched_at_epoch)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(song_key) DO UPDATE SET
            duration_secs = excluded.duration_secs,
            provider = excluded.provider,
            plain = excluded.plain,
            synced_json = excluded.synced_json,
            fetched_at_epoch = excluded.fetched_at_epoch",
    )
    .bind(song_key)
    .bind(artist)
    .bind(title)
    .bind(row.duration_secs)
    .bind(&row.provider)
    .bind(&row.plain)
    .bind(&row.synced_json)
    .bind(row.fetched_at_epoch)
    .execute(pool)
    .await
    .map(|_| ())
}

#[derive(Debug, Deserialize)]
struct LrclibRecord {
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

async fn fetch_lrclib(
    state: &AppState,
    artist: &str,
    title: &str,
    album: Option<&str>,
    duration_secs: Option<i64>,
) -> anyhow::Result<Option<Lyrics>> {
    // Exact signature lookup first; fall back to search and pick the result
    // whose duration is closest to ours.
    let mut record = lrclib_get(state, artist, title, album, duration_secs).await?;
    if record.is_none() {
        record = lrclib_search(state, artist, title, duration_secs).await?;
    }
    let Some(record) = record else {
        return Ok(None);
    };
    if record.instrumental {
        return Ok(None);
    }

    let duration_close = match (duration_secs, record.duration) {
        (Some(ours), Some(theirs)) => {
            (ours - theirs.round() as i64).abs() <= SYNC_DURATION_TOLERANCE_SECS
        }
        // Without our own duration, trust the provider's timing.
        _ => true,
    };
    let synced = record
        .synced_lyrics
        .as_deref()
        .filter(|_| duration_close)
        .map(parse_lrc)
        .filter(|lines| !lines.is_empty());
    let plain = record
        .plain_lyrics
        .filter(|text| !text.trim().is_empty())
        .map(|text| text.trim().to_string());

    if synced.is_none() && plain.is_none() {
        return Ok(None);
    }
    Ok(Some(Lyrics {
        artist: artist.to_string(),
        title: title.to_string(),
        plain,
        synced,
    }))
}

async fn lrclib_get(
    state: &AppState,
    artist: &str,
    title: &str,
    album: Option<&str>,
    duration_secs: Option<i64>,
) -> anyhow::Result<Option<LrclibRecord>> {
    let mut url = lrclib_url(state, "/api/get")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("artist_name", artist);
        query.append_pair("track_name", title);
        if let Some(album) = album {
            query.append_pair("album_name", album);
        }
        if let Some(duration) = duration_secs {
            query.append_pair("duration", &duration.to_string());
        }
    }
    let response = lrclib_request(state, url).await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let record = response.error_for_status()?.json::<LrclibRecord>().await?;
    Ok(Some(record))
}

async fn lrclib_search(
    state: &AppState,
    artist: &str,
    title: &str,
    duration_secs: Option<i64>,
) -> anyhow::Result<Option<LrclibRecord>> {
    let mut url = lrclib_url(state, "/api/search")?;
    url.query_pairs_mut()
        .append_pair("artist_name", artist)
        .append_pair("track_name", title);
    let records = lrclib_request(state, url)
        .await?
        .error_for_status()?
        .json::<Vec<LrclibRecord>>()
        .await?;
    Ok(pick_closest(records, duration_secs))
}

async fn lrclib_request(state: &AppState, url: reqwest::Url) -> anyhow::Result<reqwest::Response> {
    Ok(state
        .http
        .get(url)
        // lrclib asks bulk users to identify themselves.
        .header(
            reqwest::header::USER_AGENT,
            "songarr/0.1 (https://github.com/yesterdali/songarr)",
        )
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await?)
}

fn pick_closest(records: Vec<LrclibRecord>, duration_secs: Option<i64>) -> Option<LrclibRecord> {
    let with_lyrics = records
        .into_iter()
        .filter(|r| !r.instrumental && (r.plain_lyrics.is_some() || r.synced_lyrics.is_some()));
    match duration_secs {
        Some(ours) => with_lyrics.min_by_key(|r| {
            r.duration
                .map(|d| (ours - d.round() as i64).abs())
                .unwrap_or(i64::MAX)
        }),
        None => with_lyrics.take(1).next(),
    }
}

fn lrclib_url(state: &AppState, path: &str) -> anyhow::Result<reqwest::Url> {
    let base = state.config.lyrics.lrclib_api_base.trim_end_matches('/');
    Ok(reqwest::Url::parse(&format!("{base}{path}"))?)
}

/// Parse LRC text (`[mm:ss.xx] line`, multiple timestamps per line allowed)
/// into chronologically ordered lines.
pub fn parse_lrc(text: &str) -> Vec<SyncedLine> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(end) = rest.starts_with('[').then(|| rest.find(']')).flatten() {
            if let Some(ms) = parse_timestamp(&rest[1..end]) {
                stamps.push(ms);
                rest = rest[end + 1..].trim_start();
            } else {
                // Metadata tag like [ar:...] — skip the whole line.
                stamps.clear();
                rest = "";
                break;
            }
        }
        let value = rest.trim();
        for start_ms in stamps {
            // Keep empty lines out; they're pauses, not lyrics.
            if !value.is_empty() {
                lines.push(SyncedLine {
                    start_ms,
                    value: value.to_string(),
                });
            }
        }
    }
    lines.sort_by_key(|line| line.start_ms);
    lines
}

fn parse_timestamp(stamp: &str) -> Option<i64> {
    // mm:ss, mm:ss.xx, or mm:ss.xxx
    let (minutes, seconds) = stamp.split_once(':')?;
    let minutes: i64 = minutes.parse().ok()?;
    let (secs, frac) = match seconds.split_once('.') {
        Some((secs, frac)) => (secs, Some(frac)),
        None => (seconds, None),
    };
    let secs: i64 = secs.parse().ok()?;
    if !(0..60).contains(&secs) {
        return None;
    }
    let frac_ms = match frac {
        Some(frac) => {
            let digits: String = frac.chars().take(3).collect();
            let value: i64 = digits.parse().ok()?;
            match digits.len() {
                1 => value * 100,
                2 => value * 10,
                _ => value,
            }
        }
        None => 0,
    };
    Some(minutes * 60_000 + secs * 1_000 + frac_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc() {
        let lrc = "[ar:Янка]\n[00:12.30] Первая строка\n[00:15.9] Вторая\n[01:02] Третья";
        let lines = parse_lrc(lrc);
        assert_eq!(
            lines,
            vec![
                SyncedLine {
                    start_ms: 12_300,
                    value: "Первая строка".into()
                },
                SyncedLine {
                    start_ms: 15_900,
                    value: "Вторая".into()
                },
                SyncedLine {
                    start_ms: 62_000,
                    value: "Третья".into()
                },
            ]
        );
    }

    #[test]
    fn repeated_timestamps_expand_and_sort() {
        let lines = parse_lrc("[00:30.00][00:10.00] Припев\n[00:20.00] Куплет");
        assert_eq!(
            lines.iter().map(|l| l.start_ms).collect::<Vec<_>>(),
            vec![10_000, 20_000, 30_000]
        );
        assert_eq!(lines[0].value, "Припев");
        assert_eq!(lines[1].value, "Куплет");
    }

    #[test]
    fn empty_lines_and_metadata_are_dropped() {
        let lines = parse_lrc("[00:01.00]\n[length:3:20]\nплейн без таймстампа");
        assert!(lines.is_empty());
    }

    #[test]
    fn closest_duration_wins_search() {
        let records = vec![
            LrclibRecord {
                duration: Some(200.0),
                instrumental: false,
                plain_lyrics: Some("far".into()),
                synced_lyrics: None,
            },
            LrclibRecord {
                duration: Some(181.0),
                instrumental: false,
                plain_lyrics: Some("close".into()),
                synced_lyrics: None,
            },
        ];
        let picked = pick_closest(records, Some(180)).unwrap();
        assert_eq!(picked.plain_lyrics.as_deref(), Some("close"));
    }
}
