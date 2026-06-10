//! Virtual track → concrete YouTube URL, via `yt-dlp ytsearch` + scoring.

pub mod innertube;

use serde::Deserialize;

use crate::config::Streaming;
use crate::vtrack::VirtualTrack;

/// A search hit from `yt-dlp --flat-playlist --dump-json "ytsearchN:…"`.
#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: String,
    /// Seconds; flat extraction reports floats and sometimes null.
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub uploader: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
}

impl Candidate {
    pub fn watch_url(&self) -> String {
        self.url
            .clone()
            .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", self.id))
    }
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub url: String,
    pub score: i64,
    pub candidate_title: String,
}

/// Cached resolutions go stale (YouTube URLs and rankings drift).
const RESOLUTION_TTL_SECS: i64 = 24 * 3600;

/// Resolve with cache: a fresh stored resolution (typically prefetched at
/// search time) makes pressing play skip the whole yt-dlp search round-trip.
pub async fn resolve_cached(
    state: &crate::AppState,
    track: &crate::vtrack::VirtualTrack,
) -> anyhow::Result<Resolution> {
    if let (Some(url), Some(score), Some(at)) =
        (&track.resolved_url, track.resolved_score, track.resolved_at_epoch)
    {
        if crate::vtrack::epoch_secs() - at < RESOLUTION_TTL_SECS {
            tracing::debug!(track = track.id, "using cached resolution");
            return Ok(Resolution {
                url: url.clone(),
                score,
                candidate_title: track.resolved_title.clone().unwrap_or_default(),
            });
        }
    }
    let resolution = resolve(&state.config.streaming, track).await?;
    let _ = crate::vtrack::set_resolution(
        &state.db,
        &track.id,
        &resolution.url,
        resolution.score,
        &resolution.candidate_title,
    )
    .await;
    Ok(resolution)
}

/// Speculatively resolve freshly injected search results in the background,
/// so the likely next play starts ~2× faster. Deduped via an in-flight set
/// (incremental-search keystrokes re-inject the same tracks) and bounded by
/// the resolve gate so prefetch never starves an actual play.
pub async fn prefetch(state: crate::AppState, track_ids: Vec<String>) {
    // All injected results (≤ max_results) — the user may click any of them.
    for id in track_ids {
        let Ok(Some(track)) = crate::vtrack::get(&state.db, &id).await else {
            continue;
        };
        if track.real_subsonic_id.is_some() {
            continue;
        }
        if let Some(at) = track.resolved_at_epoch {
            if crate::vtrack::epoch_secs() - at < RESOLUTION_TTL_SECS {
                continue;
            }
        }
        {
            let mut in_flight = state.resolve_inflight.lock().await;
            if !in_flight.insert(id.clone()) {
                continue;
            }
        }
        let Ok(_permit) = state.resolve_gate.acquire().await else {
            break;
        };
        match resolve(&state.config.streaming, &track).await {
            Ok(resolution) => {
                let _ = crate::vtrack::set_resolution(
                    &state.db,
                    &id,
                    &resolution.url,
                    resolution.score,
                    &resolution.candidate_title,
                )
                .await;
                tracing::debug!(track = id, score = resolution.score, "prefetched resolution");
            }
            Err(error) => {
                tracing::debug!(%error, track = id, "prefetch resolve failed");
            }
        }
        state.resolve_inflight.lock().await.remove(&id);
    }
}

/// Run the yt-dlp search and pick the best-scoring candidate.
pub async fn resolve(streaming: &Streaming, track: &VirtualTrack) -> anyhow::Result<Resolution> {
    let query = format!("{} {}", track.artist, track.title);
    let mut command = tokio::process::Command::new(&streaming.ytdlp_path);
    command
        .arg("--dump-json")
        .arg("--flat-playlist")
        .arg("--no-warnings")
        .arg(format!("ytsearch5:{query}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if !streaming.ytdlp_proxy.is_empty() {
        command.arg("--proxy").arg(&streaming.ytdlp_proxy);
    }

    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await
        .map_err(|_| anyhow::anyhow!("yt-dlp search timed out"))??;
    anyhow::ensure!(
        output.status.success(),
        "yt-dlp search failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );

    let candidates: Vec<Candidate> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    anyhow::ensure!(!candidates.is_empty(), "yt-dlp returned no candidates");

    let best = candidates
        .iter()
        .map(|c| (score(track, c), c))
        .max_by_key(|(s, _)| *s)
        .unwrap();
    Ok(Resolution {
        url: best.1.watch_url(),
        score: best.0,
        candidate_title: best.1.title.clone(),
    })
}

/// 0–100 match score (plan M3): title/artist similarity, duration delta,
/// uploader signals, suspicious-keyword penalties.
pub fn score(track: &VirtualTrack, candidate: &Candidate) -> i64 {
    let want_title = normalize(&track.title);
    let want_artist = normalize(&track.artist);
    let got_title = normalize(&candidate.title);
    let uploader = normalize(
        candidate
            .channel
            .as_deref()
            .or(candidate.uploader.as_deref())
            .unwrap_or(""),
    );

    let mut score = 0.0;

    // Title similarity (up to 50): candidate titles often are "Artist - Title".
    let direct = strsim::jaro_winkler(&got_title, &want_title);
    let combined = strsim::jaro_winkler(&got_title, &format!("{want_artist} {want_title}"));
    score += 50.0 * direct.max(combined);

    // Artist presence (up to 25).
    if !want_artist.is_empty() && (got_title.contains(&want_artist) || uploader.contains(&want_artist))
    {
        score += 20.0;
    }
    // "Artist - Topic" auto-channels and official channels are gold.
    let raw_uploader = candidate
        .channel
        .as_deref()
        .or(candidate.uploader.as_deref())
        .unwrap_or("");
    if raw_uploader.ends_with(" - Topic") {
        score += 5.0;
    } else if normalize(raw_uploader).contains("official") {
        score += 3.0;
    }

    // Duration agreement (up to 25, slight penalty when wildly off).
    if let (Some(want_ms), Some(got)) = (track.duration_ms, candidate.duration) {
        let delta = ((want_ms as f64 / 1000.0) - got).abs();
        score += match delta {
            d if d <= 2.0 => 25.0,
            d if d <= 5.0 => 20.0,
            d if d <= 10.0 => 10.0,
            d if d <= 20.0 => 2.0,
            _ => -15.0,
        };
    } else {
        score += 5.0; // unknown duration: neutral-ish
    }

    // Suspicious keywords, unless the wanted title itself contains them.
    const SUSPECT: &[&str] = &[
        "live", "cover", "remix", "slowed", "nightcore", "8d", "spedup", "reverb", "karaoke",
        "instrumental", "reaction", "lyrics video",
    ];
    for keyword in SUSPECT {
        let k = normalize(keyword);
        if got_title.contains(&k) && !want_title.contains(&k) {
            score -= 15.0;
        }
    }

    score.clamp(0.0, 100.0).round() as i64
}

fn normalize(value: &str) -> String {
    deunicode::deunicode(value)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(artist: &str, title: &str, secs: i64) -> VirtualTrack {
        VirtualTrack {
            id: "sgr_x".into(),
            provider: "deezer".into(),
            provider_track_id: "1".into(),
            artist: artist.into(),
            title: title.into(),
            album: None,
            duration_ms: Some(secs * 1000),
            isrc: None,
            artwork_url: None,
            status: "virtual".into(),
            real_subsonic_id: None,
            resolved_url: None,
            resolved_score: None,
            resolved_title: None,
            resolved_at_epoch: None,
        }
    }

    fn candidate(title: &str, channel: &str, secs: f64) -> Candidate {
        Candidate {
            id: "vid".into(),
            url: None,
            title: title.into(),
            duration: Some(secs),
            uploader: None,
            channel: Some(channel.into()),
        }
    }

    #[test]
    fn exact_topic_match_beats_everything() {
        let want = track("Daft Punk", "One More Time", 320);
        let exact = candidate("One More Time", "Daft Punk - Topic", 320.0);
        let live = candidate("One More Time (Live at Coachella)", "randomuser", 421.0);
        let cover = candidate("One More Time - Daft Punk (Piano Cover)", "PianoGuy", 318.0);
        let s_exact = score(&want, &exact);
        let s_live = score(&want, &live);
        let s_cover = score(&want, &cover);
        assert!(s_exact > 85, "exact: {s_exact}");
        assert!(s_exact > s_live + 20, "exact {s_exact} vs live {s_live}");
        assert!(s_exact > s_cover + 20, "exact {s_exact} vs cover {s_cover}");
    }

    #[test]
    fn artist_dash_title_format_scores_high() {
        let want = track("Sigur Rós", "Hoppípolla", 268);
        let c = candidate("Sigur Rós - Hoppípolla", "Sigur Rós", 270.0);
        assert!(score(&want, &c) > 80, "{}", score(&want, &c));
    }

    #[test]
    fn wanted_remix_is_not_penalized() {
        let want = track("Artist", "Song (Remix)", 200);
        let remix = candidate("Artist - Song (Remix)", "Artist - Topic", 200.0);
        let plain = candidate("Artist - Song", "Artist - Topic", 200.0);
        assert!(score(&want, &remix) > score(&want, &plain),
            "remix {} vs plain {}", score(&want, &remix), score(&want, &plain));
    }

    #[test]
    fn wild_duration_mismatch_is_penalized() {
        let want = track("Artist", "Song", 200);
        let good = candidate("Artist - Song", "x", 201.0);
        let bad = candidate("Artist - Song", "x", 900.0);
        assert!(score(&want, &good) - score(&want, &bad) >= 30);
    }

    #[test]
    fn flat_playlist_json_parses() {
        let line = r#"{"id":"dQw4w9WgXcQ","url":"https://www.youtube.com/watch?v=dQw4w9WgXcQ","title":"Rick Astley - Never Gonna Give You Up","duration":213.0,"channel":"Rick Astley","uploader":null,"view_count":1000}"#;
        let c: Candidate = serde_json::from_str(line).unwrap();
        assert_eq!(c.watch_url(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(c.duration, Some(213.0));
        // Missing url falls back to building from id.
        let c2: Candidate = serde_json::from_str(r#"{"id":"abc","title":"x"}"#).unwrap();
        assert_eq!(c2.watch_url(), "https://www.youtube.com/watch?v=abc");
    }
}
