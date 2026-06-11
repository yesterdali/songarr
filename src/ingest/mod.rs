//! M4 — Ingest: staged stream sources become real library tracks.
//!
//! Worker loop per completed stream_job (status `finalizing`):
//!  1. ffprobe the staged source; remux (never lossy→lossy when avoidable)
//!  2. tag with lofty (artist/title/album/ISRC + songarr source comment)
//!  3. move into `<music>/<ingest_subdir>/<Artist>/<Artist> - <Title>.<ext>`
//!  4. trigger a Navidrome scan, resolve the new real id, update the mapping
//!  5. replay queued scrobbles/stars as the original user (captured tokens)
//!
//! A match score below `min_score_for_import` (or `auto_import = false`)
//! parks the job in `needs_review` with the file kept in staging (M5 UI).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::subsonic::auth;
use crate::vtrack::{self, VirtualTrack};
use crate::{jobs, pending, AppState};

/// Long-running worker; spawn once at startup.
pub async fn worker(state: AppState) {
    let interval = Duration::from_secs(state.config.ingest.poll_secs.max(1));
    loop {
        if let Err(error) = tick(&state).await {
            tracing::error!(%error, "ingest worker tick failed");
        }
        tokio::time::sleep(interval).await;
    }
}

async fn tick(state: &AppState) -> anyhow::Result<()> {
    let ready: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM stream_jobs WHERE status = 'finalizing' ORDER BY created_at LIMIT 5",
    )
    .fetch_all(&state.db)
    .await?;
    for (job_id,) in ready {
        // Claim atomically; lost claims mean another worker instance got it.
        let claimed = sqlx::query(
            "UPDATE stream_jobs SET status = 'importing' WHERE id = ? AND status = 'finalizing'",
        )
        .bind(&job_id)
        .execute(&state.db)
        .await?
        .rows_affected();
        if claimed == 0 {
            continue;
        }
        match ingest_job(state, &job_id).await {
            Ok(IngestOutcome::Imported { real_id }) => {
                tracing::info!(job = job_id, real_id, "ingest complete");
            }
            Ok(IngestOutcome::NeedsReview { reason }) => {
                tracing::info!(job = job_id, reason, "ingest parked for review");
            }
            Err(error) => {
                tracing::error!(%error, job = job_id, "ingest failed");
                let _ =
                    jobs::set_status(&state.db, &job_id, "failed", Some(&error.to_string())).await;
            }
        }
    }
    Ok(())
}

pub enum IngestOutcome {
    Imported { real_id: String },
    NeedsReview { reason: String },
}

#[derive(sqlx::FromRow)]
struct JobRow {
    virtual_track_id: String,
    staging_path: Option<String>,
    match_score: Option<i64>,
    source_url: Option<String>,
}

/// Ingest one claimed job end-to-end. Public so the M5 admin UI can run a
/// needs_review item on demand (`force` skips the score/auto gates).
pub async fn ingest_job(state: &AppState, job_id: &str) -> anyhow::Result<IngestOutcome> {
    ingest_job_inner(state, job_id, false).await
}

pub async fn ingest_job_forced(state: &AppState, job_id: &str) -> anyhow::Result<IngestOutcome> {
    ingest_job_inner(state, job_id, true).await
}

async fn ingest_job_inner(
    state: &AppState,
    job_id: &str,
    force: bool,
) -> anyhow::Result<IngestOutcome> {
    let job: JobRow = sqlx::query_as(
        "SELECT virtual_track_id, staging_path, match_score, source_url
         FROM stream_jobs WHERE id = ?",
    )
    .bind(job_id)
    .fetch_one(&state.db)
    .await?;
    let track = vtrack::get(&state.db, &job.virtual_track_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("virtual track {} missing", job.virtual_track_id))?;

    // Another job may have imported this track already.
    if let Some(real_id) = &track.real_subsonic_id {
        if let Some(staging) = &job.staging_path {
            let _ = tokio::fs::remove_file(staging).await;
        }
        jobs::set_status(&state.db, job_id, "imported", None).await?;
        return Ok(IngestOutcome::Imported {
            real_id: real_id.clone(),
        });
    }

    let staging_path = job
        .staging_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("job has no staging path"))?;
    anyhow::ensure!(
        tokio::fs::try_exists(staging_path).await.unwrap_or(false),
        "staged file {staging_path} is gone"
    );

    let score = job.match_score.unwrap_or(0);
    if !force {
        let gate_reason = if !state.config.ingest.auto_import {
            Some("auto_import disabled".to_string())
        } else if score < i64::from(state.config.ingest.min_score_for_import) {
            Some(format!(
                "match score {score} below threshold {}",
                state.config.ingest.min_score_for_import
            ))
        } else {
            None
        };
        if let Some(reason) = gate_reason {
            jobs::set_status(&state.db, job_id, "needs_review", Some(&reason)).await?;
            return Ok(IngestOutcome::NeedsReview { reason });
        }
    }

    // 1+2. Remux to the final container and tag.
    let final_file = remux(staging_path, &state.config.library.staging_dir, job_id).await?;
    tag_file(&final_file, &track, job.source_url.as_deref())?;

    // 3. Move into the library.
    let dest = library_destination(
        &state.config.library.music_dir,
        &state.config.library.ingest_subdir,
        &track,
        final_file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("opus"),
    );
    tokio::fs::create_dir_all(dest.parent().unwrap()).await?;
    move_file(&final_file, &dest).await?;
    tracing::info!(dest = %dest.display(), "moved into library");

    // 4. Scan + resolve the real id.
    trigger_scan_and_wait(state).await?;
    let real_id = find_real_id(state, &track).await?;
    vtrack::mark_imported(&state.db, &track.id, &real_id).await?;
    jobs::set_status(&state.db, job_id, "imported", None).await?;
    let _ = tokio::fs::remove_file(staging_path).await;

    // 5. Replay pending scrobbles/stars; failures are logged and dropped.
    replay_pending(state, &track.id, &real_id).await;

    // Optional rolling playlist of imports.
    write_playlist_entry(state, &dest).await;

    Ok(IngestOutcome::Imported { real_id })
}

/// ffprobe the source codec and remux into a sane final container,
/// copying the stream whenever the codec is one Navidrome handles natively.
async fn remux(source: &str, scratch_dir: &Path, job_id: &str) -> anyhow::Result<PathBuf> {
    let codec = ffprobe_codec(source).await?;
    let (ext, codec_args): (&str, &[&str]) = match codec.as_str() {
        "opus" => ("opus", &["-c:a", "copy"]),
        "vorbis" => ("ogg", &["-c:a", "copy"]),
        "aac" | "alac" => ("m4a", &["-c:a", "copy", "-movflags", "+faststart"]),
        "mp3" => ("mp3", &["-c:a", "copy"]),
        "flac" => ("flac", &["-c:a", "copy"]),
        // Unknown codec: transcode once to opus (unavoidable lossy→lossy).
        other => {
            tracing::warn!(codec = other, "unknown source codec; transcoding to opus");
            ("opus", &["-c:a", "libopus", "-b:a", "160k"])
        }
    };

    let target = scratch_dir.join(format!("{job_id}.final.{ext}"));
    let output = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            source,
            "-vn",
            "-map",
            "a",
        ])
        .args(codec_args)
        .arg(&target)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "ffmpeg remux failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(target)
}

async fn ffprobe_codec(path: &str) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let codec = String::from_utf8_lossy(&output.stdout).trim().to_string();
    anyhow::ensure!(!codec.is_empty(), "no audio stream in {path}");
    Ok(codec)
}

fn tag_file(path: &Path, track: &VirtualTrack, source_url: Option<&str>) -> anyhow::Result<()> {
    use lofty::config::WriteOptions;
    use lofty::file::TaggedFileExt;
    use lofty::prelude::*;
    use lofty::tag::{ItemKey, Tag};

    let mut tagged = lofty::probe::Probe::open(path)?.read()?;
    if tagged.primary_tag_mut().is_none() {
        let tag_type = tagged.primary_tag_type();
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_artist(track.artist.clone());
    tag.set_title(track.title.clone());
    if let Some(album) = &track.album {
        tag.set_album(album.clone());
    }
    if let Some(isrc) = &track.isrc {
        tag.insert_text(ItemKey::Isrc, isrc.clone());
    }
    let source = source_url.unwrap_or("unknown");
    tag.insert_text(ItemKey::Comment, format!("songarr:src={source}"));
    tagged.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

fn library_destination(
    music_dir: &Path,
    ingest_subdir: &str,
    track: &VirtualTrack,
    ext: &str,
) -> PathBuf {
    let artist = sanitize_filename(&track.artist);
    let title = sanitize_filename(&track.title);
    let mut dest = music_dir
        .join(ingest_subdir)
        .join(&artist)
        .join(format!("{artist} - {title}.{ext}"));
    if dest.exists() {
        let tail = &uuid::Uuid::new_v4().simple().to_string()[..6];
        dest = dest.with_file_name(format!("{artist} - {title} [{tail}].{ext}"));
    }
    dest
}

/// Conservative cross-platform sanitization for path components.
pub fn sanitize_filename(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let mut result = trimmed.to_string();
    if result.len() > 120 {
        let mut cut = 120;
        while !result.is_char_boundary(cut) {
            cut -= 1;
        }
        result.truncate(cut);
    }
    if result.is_empty() {
        result = "Unknown".into();
    }
    result
}

/// rename(2) when possible, copy+remove across filesystems.
async fn move_file(from: &Path, to: &Path) -> anyhow::Result<()> {
    if tokio::fs::rename(from, to).await.is_ok() {
        return Ok(());
    }
    tokio::fs::copy(from, to).await?;
    tokio::fs::remove_file(from).await?;
    Ok(())
}

async fn trigger_scan_and_wait(state: &AppState) -> anyhow::Result<()> {
    let base = state.config.navidrome.base_url.trim_end_matches('/');
    let start_url = format!(
        "{base}/rest/startScan?{}&f=json",
        auth::admin_auth_query(&state.config.navidrome)
    );
    let response: serde_json::Value = state.http.get(&start_url).send().await?.json().await?;
    anyhow::ensure!(
        response["subsonic-response"]["status"] == "ok",
        "startScan failed: {response}"
    );

    for _ in 0..120 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let status_url = format!(
            "{base}/rest/getScanStatus?{}&f=json",
            auth::admin_auth_query(&state.config.navidrome)
        );
        let response: serde_json::Value = state.http.get(&status_url).send().await?.json().await?;
        if response["subsonic-response"]["scanStatus"]["scanning"] == false {
            return Ok(());
        }
    }
    anyhow::bail!("navidrome scan did not finish within 60s")
}

/// Find the imported file's real subsonic id via search3 (admin creds),
/// preferring hits whose path lives under the ingest subdir.
async fn find_real_id(state: &AppState, track: &VirtualTrack) -> anyhow::Result<String> {
    let base = state.config.navidrome.base_url.trim_end_matches('/');
    let query: String = url::form_urlencoded::byte_serialize(
        format!("{} {}", track.artist, track.title).as_bytes(),
    )
    .collect();

    for attempt in 0..10 {
        let url = format!(
            "{base}/rest/search3?{}&f=json&songCount=40&query={query}",
            auth::admin_auth_query(&state.config.navidrome)
        );
        let response: serde_json::Value = state.http.get(&url).send().await?.json().await?;
        let songs = response["subsonic-response"]["searchResult3"]["song"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let matches: Vec<&serde_json::Value> = songs
            .iter()
            .filter(|s| {
                normalize(s["artist"].as_str().unwrap_or("")) == normalize(&track.artist)
                    && normalize(s["title"].as_str().unwrap_or("")) == normalize(&track.title)
            })
            .collect();
        let preferred = matches
            .iter()
            .find(|s| {
                s["path"]
                    .as_str()
                    .map(|p| p.contains(&state.config.library.ingest_subdir))
                    .unwrap_or(false)
            })
            .or_else(|| matches.first());
        if let Some(song) = preferred {
            return Ok(song["id"].as_str().unwrap_or_default().to_string());
        }
        tokio::time::sleep(Duration::from_millis(500 + attempt * 200)).await;
    }
    anyhow::bail!(
        "imported track '{} — {}' not found in navidrome after scan",
        track.artist,
        track.title
    )
}

fn normalize(value: &str) -> String {
    deunicode::deunicode(value)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Replay stored scrobbles/stars against the real id using the captured
/// original query params (the user's own token). Failures are dropped.
async fn replay_pending(state: &AppState, virtual_track_id: &str, real_id: &str) {
    let actions = match pending::for_track(&state.db, virtual_track_id).await {
        Ok(actions) => actions,
        Err(error) => {
            tracing::error!(%error, "failed to load pending actions");
            return;
        }
    };
    let base = state
        .config
        .navidrome
        .base_url
        .trim_end_matches('/')
        .to_string();

    for action in actions {
        let replayed = async {
            let payload: serde_json::Value =
                serde_json::from_str(action.payload_json.as_deref().unwrap_or("{}"))?;
            let endpoint = payload["endpoint"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("payload missing endpoint"))?;
            let original_query = payload["query"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("payload missing query"))?;

            let new_query = {
                let mut serializer = url::form_urlencoded::Serializer::new(String::new());
                for (key, value) in url::form_urlencoded::parse(original_query.as_bytes()) {
                    if key == "id" {
                        serializer.append_pair("id", real_id);
                    } else if key == "f" {
                        continue;
                    } else {
                        serializer.append_pair(&key, &value);
                    }
                }
                serializer.append_pair("f", "json");
                serializer.finish()
            };
            let url = format!("{base}/rest/{endpoint}?{new_query}");
            let response: serde_json::Value = state.http.get(&url).send().await?.json().await?;
            anyhow::ensure!(
                response["subsonic-response"]["status"] == "ok",
                "navidrome rejected replay: {response}"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;

        match replayed {
            Ok(()) => tracing::info!(
                action = action.action,
                user = action.username,
                real_id,
                "replayed pending action"
            ),
            Err(error) => tracing::warn!(
                %error,
                action = action.action,
                "pending action replay failed; dropping"
            ),
        }
        let _ = pending::delete(&state.db, &action.id).await;
    }
}

async fn write_playlist_entry(state: &AppState, imported: &Path) {
    let playlist_name = &state.config.ingest.write_playlist;
    if playlist_name.is_empty() {
        return;
    }
    let playlist_path = state.config.library.music_dir.join(playlist_name);
    let line = imported
        .strip_prefix(&state.config.library.music_dir)
        .unwrap_or(imported)
        .to_string_lossy()
        .to_string();
    let entry = format!("{line}\n");
    let result = async {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&playlist_path)
            .await?;
        file.write_all(entry.as_bytes()).await
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, playlist = %playlist_path.display(), "playlist append failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_handles_separators_and_reserved_chars() {
        assert_eq!(sanitize_filename("AC/DC"), "AC_DC");
        assert_eq!(
            sanitize_filename("What? When: Where*"),
            "What_ When_ Where_"
        );
        assert_eq!(sanitize_filename("...dots..."), "dots");
        assert_eq!(sanitize_filename("   "), "Unknown");
        assert_eq!(sanitize_filename("Ünïcødé"), "Ünïcødé");
        assert!(sanitize_filename(&"x".repeat(500)).len() <= 120);
    }

    #[test]
    fn destination_layout_matches_plan() {
        let track = VirtualTrack {
            id: "sgr_x".into(),
            provider: "deezer".into(),
            provider_track_id: "1".into(),
            artist: "Mock Artist".into(),
            title: "Mock Song One".into(),
            album: None,
            duration_ms: None,
            isrc: None,
            artwork_url: None,
            status: "staged".into(),
            real_subsonic_id: None,
            resolved_url: None,
            resolved_score: None,
            resolved_title: None,
            resolved_at_epoch: None,
        };
        let dest = library_destination(Path::new("/music"), "_songarr", &track, "opus");
        assert_eq!(
            dest,
            PathBuf::from("/music/_songarr/Mock Artist/Mock Artist - Mock Song One.opus")
        );
    }
}
