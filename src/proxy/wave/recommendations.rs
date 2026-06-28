use super::*;

#[derive(Debug, Serialize)]
pub(super) struct WaveNextResponse {
    pub(super) tracks: Vec<WaveTrack>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WaveTrack {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) provider: Option<String>,
    pub(crate) reason: Option<WaveReason>,
    pub(crate) artist_id: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) album_id: Option<String>,
    pub(crate) duration_secs: Option<i64>,
    pub(crate) cover_art: Option<String>,
    pub(crate) stream_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WaveReason {
    kind: &'static str,
    source: Option<&'static str>,
    seed_artist: Option<String>,
    seed_title: Option<String>,
}

impl WaveReason {
    fn source(kind: &'static str, source: &'static str) -> Self {
        Self {
            kind,
            source: Some(source),
            seed_artist: None,
            seed_title: None,
        }
    }

    fn seed(kind: &'static str, source: Option<&'static str>, seed: &SeedTrack) -> Self {
        Self {
            kind,
            source,
            seed_artist: Some(seed.artist.clone()),
            seed_title: Some(seed.title.clone()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackRequest {
    pub(super) track_id: String,
    pub(super) action: String,
}

pub(super) async fn next_tracks(
    state: &AppState,
    username: &str,
    auth_stream: &str,
    auth_json: &str,
    seed_id: Option<String>,
    count: usize,
) -> anyhow::Result<Vec<WaveTrack>> {
    let suppression = FeedbackSuppression::load(state, username).await;
    let mut tracks = Vec::new();
    let mut existing = Vec::new();

    if let Some(seed_id) = seed_id {
        if let Ok(seed) = seed_from_id(state, &seed_id).await {
            extend_from_seed(
                state,
                username,
                auth_stream,
                &suppression,
                &mut existing,
                &mut tracks,
                seed,
                count,
                "similar_to_current",
            )
            .await;
        }
    } else {
        for seed in positive_feedback_seeds(state, username, 2).await? {
            let remaining = count.saturating_sub(tracks.len());
            if remaining == 0 {
                break;
            }
            extend_from_seed(
                state,
                username,
                auth_stream,
                &suppression,
                &mut existing,
                &mut tracks,
                seed,
                remaining.min(6),
                "because_liked",
            )
            .await;
        }
        for seed in crate::recs::recent_listen_seeds(&state.db, username, 2).await? {
            let remaining = count.saturating_sub(tracks.len());
            if remaining == 0 {
                break;
            }
            let seed = listen_seed_to_seed_track(state, seed).await;
            extend_from_seed(
                state,
                username,
                auth_stream,
                &suppression,
                &mut existing,
                &mut tracks,
                seed,
                remaining.min(6),
                "because_played",
            )
            .await;
        }
    }

    if tracks.len() < count {
        let remaining = count - tracks.len();
        extend_from_yandex_wave(
            state,
            username,
            auth_stream,
            &suppression,
            &mut existing,
            &mut tracks,
            remaining,
        )
        .await;
    }

    if tracks.len() < count && crate::yandex::available(&state.config.yandex) {
        let remaining = count - tracks.len();
        let cached_target = remaining.min((count / 2).max(1));
        let target_count = tracks.len() + cached_target;
        extend_from_cached_provider(
            state,
            auth_stream,
            &suppression,
            &mut existing,
            &mut tracks,
            crate::yandex::PROVIDER,
            target_count,
            remaining.saturating_mul(3).max(cached_target),
        )
        .await;
    }

    if tracks.len() < count {
        let remaining = count - tracks.len();
        let sample_count = remaining
            .saturating_mul(8)
            .max(remaining.max(24))
            .min(MAX_RANDOM_FALLBACK_COUNT);
        extend_from_random(
            state,
            auth_stream,
            auth_json,
            &suppression,
            &mut existing,
            &mut tracks,
            count,
            sample_count,
            true,
        )
        .await;
    }

    if tracks.len() < count {
        extend_from_random(
            state,
            auth_stream,
            auth_json,
            &suppression,
            &mut existing,
            &mut tracks,
            count,
            MAX_RANDOM_FALLBACK_COUNT,
            false,
        )
        .await;
    }

    Ok(tracks)
}

async fn extend_from_cached_provider(
    state: &AppState,
    auth_stream: &str,
    suppression: &FeedbackSuppression,
    existing: &mut Vec<crate::proxy::search::SongKey>,
    tracks: &mut Vec<WaveTrack>,
    provider: &str,
    target_count: usize,
    sample_count: usize,
) {
    let cached = match vtrack::random_by_provider(&state.db, provider, sample_count).await {
        Ok(cached) => cached,
        Err(error) => {
            tracing::debug!(%error, provider, "wave cached provider fallback failed");
            return;
        }
    };
    for track in cached {
        if suppression.is_suppressed(&track.artist, &track.title) {
            continue;
        }
        let key = crate::recs::song_key(&track.artist, &track.title);
        let track_key =
            crate::proxy::search::SongKey::new(&track.artist, &track.title, track.duration_ms);
        if existing.iter().any(|existing| existing.matches(&track_key)) {
            continue;
        }
        if tracks
            .iter()
            .any(|t| t.id == track.id || crate::recs::song_key(&t.artist, &t.title) == key)
        {
            continue;
        }
        existing.push(track_key);
        let mut wave_track = wave_track_from_entry(
            SongEntry::from_virtual(&track, &state.config.streaming),
            auth_stream,
        );
        wave_track.reason = Some(WaveReason::source("yandex_cache", "yandex"));
        tracks.push(wave_track);
        if tracks.len() >= target_count {
            break;
        }
    }
}

async fn extend_from_random(
    state: &AppState,
    auth_stream: &str,
    auth_json: &str,
    suppression: &FeedbackSuppression,
    existing: &mut Vec<crate::proxy::search::SongKey>,
    tracks: &mut Vec<WaveTrack>,
    target_count: usize,
    sample_count: usize,
    respect_suppression: bool,
) {
    match random_tracks(state, auth_stream, auth_json, sample_count).await {
        Ok(random) => {
            for track in random {
                let key = crate::recs::song_key(&track.artist, &track.title);
                let track_key = crate::proxy::search::SongKey::new(
                    &track.artist,
                    &track.title,
                    track.duration_secs,
                );
                if respect_suppression && suppression.is_suppressed(&track.artist, &track.title) {
                    continue;
                }
                if existing.iter().any(|existing| existing.matches(&track_key)) {
                    continue;
                }
                if tracks
                    .iter()
                    .any(|t| t.id == track.id || crate::recs::song_key(&t.artist, &t.title) == key)
                {
                    continue;
                }
                existing.push(track_key);
                let mut track = track;
                track.reason = Some(WaveReason::source("library_random", "library"));
                tracks.push(track);
                if tracks.len() >= target_count {
                    break;
                }
            }
        }
        Err(error) => tracing::debug!(%error, sample_count, "wave random fallback failed"),
    }
}

async fn extend_from_yandex_wave(
    state: &AppState,
    username: &str,
    auth_stream: &str,
    suppression: &FeedbackSuppression,
    existing: &mut Vec<crate::proxy::search::SongKey>,
    tracks: &mut Vec<WaveTrack>,
    count: usize,
) {
    let cfg = &state.config.recommendations;
    if count == 0 || cfg.weight_yandex <= 0.0 || !crate::yandex::available(&state.config.yandex) {
        return;
    }
    let fetch_limit = count.saturating_mul(2).clamp(count, MAX_NEXT_COUNT);
    let seed_key = format!("user:{username}");
    let candidates = match crate::recs::cache_get(
        &state.db,
        "yandex_wave",
        &seed_key,
        cfg.cache_ttl_hours,
    )
    .await
    {
        Ok(Some(cached)) => cached,
        Ok(None) => match crate::recs::yandex::wave(&state.config.yandex, fetch_limit).await {
            Ok(candidates) => {
                let _ =
                    crate::recs::cache_set(&state.db, "yandex_wave", &seed_key, &candidates).await;
                candidates
            }
            Err(error) => {
                tracing::debug!(%error, username, "Yandex wave source abstained");
                return;
            }
        },
        Err(error) => {
            tracing::debug!(%error, username, "Yandex wave cache read failed");
            return;
        }
    };

    let entries = match crate::proxy::similar::upsert_candidates(
        state,
        username,
        candidates,
        fetch_limit,
        existing,
    )
    .await
    {
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(%error, username, "Yandex wave upsert failed");
            return;
        }
    };

    let target_len = tracks.len() + count;
    for entry in entries {
        if suppression.is_suppressed(&entry.artist, &entry.title) {
            continue;
        }
        let key =
            crate::proxy::search::SongKey::new(&entry.artist, &entry.title, entry.duration_secs);
        if tracks.iter().any(|t| t.id == entry.id) || existing.iter().any(|e| e.matches(&key)) {
            continue;
        }
        existing.push(key);
        let mut track = wave_track_from_entry(entry, auth_stream);
        track.provider = Some(crate::yandex::PROVIDER.into());
        track.reason = Some(WaveReason::source("yandex_wave", "yandex"));
        tracks.push(track);
        if tracks.len() >= target_len {
            break;
        }
    }
}

async fn extend_from_seed(
    state: &AppState,
    username: &str,
    auth_stream: &str,
    suppression: &FeedbackSuppression,
    existing: &mut Vec<crate::proxy::search::SongKey>,
    tracks: &mut Vec<WaveTrack>,
    seed: SeedTrack,
    count: usize,
    reason_kind: &'static str,
) {
    let reason = WaveReason::seed(reason_kind, source_from_provider(&seed.provider), &seed);
    let request_count = count.saturating_mul(2).clamp(count, MAX_NEXT_COUNT);
    let entries = match tokio::time::timeout(
        std::time::Duration::from_secs(SEED_EXTENSION_TIMEOUT_SECS),
        crate::proxy::similar::recommended_for_seed(
            state,
            username,
            &seed,
            request_count,
            existing,
        ),
    )
    .await
    {
        Ok(Ok(entries)) => entries,
        Ok(Err(error)) => {
            tracing::debug!(%error, artist = seed.artist, title = seed.title, "wave seed produced no recommendations");
            Vec::new()
        }
        Err(error) => {
            tracing::debug!(%error, artist = seed.artist, title = seed.title, "wave seed timed out");
            Vec::new()
        }
    };
    for entry in entries {
        if suppression.is_suppressed(&entry.artist, &entry.title) {
            continue;
        }
        existing.push(crate::proxy::search::SongKey::new(
            &entry.artist,
            &entry.title,
            entry.duration_secs,
        ));
        if !tracks.iter().any(|t| t.id == entry.id) {
            let mut track = wave_track_from_entry(entry, auth_stream);
            track.reason = Some(reason.clone());
            tracks.push(track);
        }
        if tracks.len() >= count {
            break;
        }
    }
}

async fn seed_from_id(state: &AppState, id: &str) -> anyhow::Result<SeedTrack> {
    if vtrack::is_virtual_id(id) {
        let track = vtrack::get(&state.db, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("virtual seed not found"))?;
        return Ok(SeedTrack::from(track));
    }
    crate::proxy::similar::fetch_real_seed(state, id).await
}

async fn listen_seed_to_seed_track(state: &AppState, seed: crate::recs::ListenSeed) -> SeedTrack {
    if let Some(id) = &seed.subsonic_id {
        if let Ok(seed) = seed_from_id(state, id).await {
            return seed;
        }
    }
    SeedTrack {
        artist: seed.artist,
        title: seed.title,
        provider: "listen".into(),
        provider_track_id: seed
            .subsonic_id
            .unwrap_or_else(|| format!("listen:{}", uuid::Uuid::new_v4())),
    }
}

fn source_from_provider(provider: &str) -> Option<&'static str> {
    match provider {
        "yandex" => Some("yandex"),
        "ytmusic" | "ytm" => Some("ytm"),
        "deezer" => Some("deezer"),
        "lastfm" => Some("lastfm"),
        "listen" | "feedback" => Some("library"),
        _ => None,
    }
}

async fn positive_feedback_seeds(
    state: &AppState,
    username: &str,
    limit: usize,
) -> sqlx::Result<Vec<SeedTrack>> {
    let rows: Vec<(Option<String>, String, String)> = sqlx::query_as(
        "SELECT track_id, artist, title
         FROM wave_feedback
         WHERE username = ? AND action IN ('like', 'play')
         ORDER BY created_at_epoch DESC
         LIMIT ?",
    )
    .bind(username)
    .bind(limit as i64 * 3)
    .fetch_all(&state.db)
    .await?;
    let mut seen = std::collections::HashSet::new();
    let mut seeds = Vec::new();
    for (track_id, artist, title) in rows {
        let key = crate::recs::song_key(&artist, &title);
        if !seen.insert(key) {
            continue;
        }
        let seed = match track_id.as_deref() {
            Some(id) => seed_from_id(state, id).await.ok(),
            None => None,
        }
        .unwrap_or_else(|| SeedTrack {
            artist,
            title,
            provider: "feedback".into(),
            provider_track_id: track_id.unwrap_or_else(|| "feedback".into()),
        });
        seeds.push(seed);
        if seeds.len() >= limit {
            break;
        }
    }
    Ok(seeds)
}

async fn random_tracks(
    state: &AppState,
    auth_stream: &str,
    auth_json: &str,
    count: usize,
) -> anyhow::Result<Vec<WaveTrack>> {
    let url = format!(
        "{}/rest/getRandomSongs?{auth_json}&size={count}",
        state.config.navidrome.base_url.trim_end_matches('/')
    );
    let value: serde_json::Value = state
        .http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    anyhow::ensure!(
        value["subsonic-response"]["status"].as_str() == Some("ok"),
        "random songs failed"
    );
    let songs = &value["subsonic-response"]["randomSongs"]["song"];
    let items: Vec<serde_json::Value> = match songs {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(_) => vec![songs.clone()],
        _ => Vec::new(),
    };
    Ok(items
        .into_iter()
        .filter_map(|song| wave_track_from_json(&song, auth_stream))
        .collect())
}

pub(super) async fn record_feedback(
    state: &AppState,
    username: &str,
    track_id: &str,
    action: &str,
) -> anyhow::Result<()> {
    let seed = seed_from_id(state, track_id).await?;
    let now = vtrack::epoch_secs();
    let song_key = crate::recs::song_key(&seed.artist, &seed.title);
    let artist_key = crate::recs::artist_key(&seed.artist);
    sqlx::query(
        "INSERT INTO wave_feedback
            (username, track_id, artist, title, song_key, artist_key, action, created_at_epoch)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(username, song_key, action) DO UPDATE SET
            track_id = excluded.track_id,
            artist = excluded.artist,
            title = excluded.title,
            artist_key = excluded.artist_key,
            created_at_epoch = excluded.created_at_epoch",
    )
    .bind(username)
    .bind(track_id)
    .bind(&seed.artist)
    .bind(&seed.title)
    .bind(&song_key)
    .bind(&artist_key)
    .bind(action)
    .bind(now)
    .execute(&state.db)
    .await?;
    if action == "play" {
        crate::recs::record_listen(
            &state.db,
            username,
            &seed.artist,
            &seed.title,
            Some(track_id),
            now,
        )
        .await?;
    }
    Ok(())
}

#[derive(Default)]
struct FeedbackSuppression {
    song_keys: std::collections::HashSet<String>,
    artist_keys: std::collections::HashSet<String>,
}

impl FeedbackSuppression {
    async fn load(state: &AppState, username: &str) -> Self {
        let now = vtrack::epoch_secs();
        let cutoff = now - FEEDBACK_DISLIKE_COOLDOWN_DAYS * 86_400;
        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT song_key, artist_key, action, created_at_epoch
             FROM wave_feedback
             WHERE username = ? AND action IN ('skip', 'dislike') AND created_at_epoch >= ?",
        )
        .bind(username)
        .bind(cutoff)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        let mut suppression = Self::default();
        for (song_key, artist_key, action, created_at) in rows {
            match action.as_str() {
                "dislike" => {
                    suppression.song_keys.insert(song_key);
                    suppression.artist_keys.insert(artist_key);
                }
                "skip" if created_at >= now - FEEDBACK_SKIP_COOLDOWN_DAYS * 86_400 => {
                    suppression.song_keys.insert(song_key);
                }
                _ => {}
            }
        }
        suppression
    }

    fn is_suppressed(&self, artist: &str, title: &str) -> bool {
        self.song_keys
            .contains(&crate::recs::song_key(artist, title))
            || self.artist_keys.contains(&crate::recs::artist_key(artist))
    }
}

fn wave_track_from_entry(entry: SongEntry, auth_stream: &str) -> WaveTrack {
    WaveTrack {
        stream_url: stream_url(auth_stream, &entry.id),
        id: entry.id,
        title: entry.title,
        artist: entry.artist,
        provider: entry.provider,
        reason: None,
        artist_id: None,
        album: entry.album,
        album_id: None,
        duration_secs: entry.duration_secs,
        cover_art: entry.cover_art,
    }
}

fn wave_track_from_json(song: &serde_json::Value, auth_stream: &str) -> Option<WaveTrack> {
    let id = song["id"].as_str()?.to_string();
    Some(WaveTrack {
        stream_url: stream_url(auth_stream, &id),
        id,
        title: song["title"].as_str().unwrap_or("Unknown").to_string(),
        artist: song["artist"]
            .as_str()
            .unwrap_or("Unknown artist")
            .to_string(),
        provider: None,
        reason: None,
        artist_id: song["artistId"].as_str().map(str::to_string),
        album: song["album"].as_str().map(str::to_string),
        album_id: song["albumId"].as_str().map(str::to_string),
        duration_secs: song["duration"].as_i64(),
        cover_art: song["coverArt"].as_str().map(str::to_string),
    })
}

fn stream_url(auth_stream: &str, id: &str) -> String {
    format!("/rest/stream?{auth_stream}&id={}", percent_encode(id))
}
