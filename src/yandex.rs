//! JSON bridge to the Python Yandex Music helper.
//!
//! The Yandex Music API we use is unofficial and much easier to keep up to
//! date in Python via `yandex-music-api`. Rust only depends on the stable JSON
//! contract below, which tests can mock without real Yandex credentials.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::config::Yandex as YandexConfig;

pub const PROVIDER: &str = "yandex";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YandexTrack {
    #[serde(alias = "track_id", alias = "trackId", alias = "id")]
    pub track_id: String,
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YandexDownload {
    pub url: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub bitrate_kbps: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest<'a> {
    access_token: &'a str,
    refresh_token: &'a str,
    query: &'a str,
    limit: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WaveRequest<'a> {
    access_token: &'a str,
    refresh_token: &'a str,
    limit: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequest<'a> {
    access_token: &'a str,
    refresh_token: &'a str,
    track_id: &'a str,
}

pub fn available(config: &YandexConfig) -> bool {
    config.enabled && !config.access_token.trim().is_empty()
}

pub async fn search(
    config: &YandexConfig,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<YandexTrack>> {
    if !available(config) || !config.use_for_search {
        return Ok(Vec::new());
    }
    run_helper(
        config,
        "search",
        &SearchRequest {
            access_token: &config.access_token,
            refresh_token: &config.refresh_token,
            query,
            limit,
        },
    )
    .await
}

pub async fn wave(config: &YandexConfig, limit: usize) -> anyhow::Result<Vec<YandexTrack>> {
    if !available(config) || !config.use_for_wave {
        return Ok(Vec::new());
    }
    run_helper_with_timeout(
        config,
        "wave",
        &WaveRequest {
            access_token: &config.access_token,
            refresh_token: &config.refresh_token,
            limit,
        },
        // Wave is an interactive "give me something now" path. Search and
        // import use the configured timeout, but personalized Yandex radio can
        // be slow or unavailable, so cap it and let other providers/fallbacks
        // keep the queue alive.
        Duration::from_secs(config.api_timeout_secs.clamp(1, 5)),
    )
    .await
}

pub async fn download(config: &YandexConfig, track_id: &str) -> anyhow::Result<YandexDownload> {
    anyhow::ensure!(
        available(config) && config.use_for_import,
        "Yandex import disabled or token missing"
    );
    run_helper(
        config,
        "download",
        &DownloadRequest {
            access_token: &config.access_token,
            refresh_token: &config.refresh_token,
            track_id,
        },
    )
    .await
}

async fn run_helper<T: DeserializeOwned, P: Serialize>(
    config: &YandexConfig,
    command: &str,
    payload: &P,
) -> anyhow::Result<T> {
    run_helper_with_timeout(
        config,
        command,
        payload,
        Duration::from_secs(config.api_timeout_secs.max(1)),
    )
    .await
}

async fn run_helper_with_timeout<T: DeserializeOwned, P: Serialize>(
    config: &YandexConfig,
    command: &str,
    payload: &P,
    timeout: Duration,
) -> anyhow::Result<T> {
    let input = serde_json::to_vec(payload)?;
    let (program, mut args) = helper_invocation(&config.helper_path);
    args.push(command.into());
    let mut child = tokio::process::Command::new(&program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| anyhow::anyhow!("starting Yandex helper failed: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&input).await?;
    }

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("Yandex helper timed out after {}s", timeout.as_secs()))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Yandex helper {command} failed: {}", stderr.trim());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("invalid Yandex helper JSON: {error}"))
}

fn helper_invocation(helper_path: &str) -> (PathBuf, Vec<String>) {
    let helper = PathBuf::from(helper_path);
    let is_source_helper = helper.ends_with("scripts/songarr-yandex");
    if !is_source_helper {
        return (helper, Vec::new());
    }
    let python = std::env::var("VIRTUAL_ENV")
        .map(|venv| PathBuf::from(venv).join("bin/python"))
        .ok()
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("python3"));
    (python, vec![helper_path.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_helper_track_aliases() {
        let track: YandexTrack = serde_json::from_value(serde_json::json!({
            "id": "123",
            "artist": "Янка",
            "title": "Нюркина песня",
            "album": "Стыд и срам",
            "durationMs": 181000,
            "artworkUrl": "https://example.test/art.jpg"
        }))
        .unwrap();
        assert_eq!(track.track_id, "123");
        assert_eq!(track.duration_ms, Some(181000));
        assert_eq!(
            track.artwork_url.as_deref(),
            Some("https://example.test/art.jpg")
        );
    }

    #[test]
    fn unavailable_without_token() {
        let config = YandexConfig {
            enabled: true,
            ..YandexConfig::default()
        };
        assert!(!available(&config));
    }

    #[test]
    fn source_helper_runs_through_python() {
        let (program, args) = helper_invocation("scripts/songarr-yandex");
        assert!(program.ends_with("python") || program.ends_with("python3"));
        assert_eq!(args, vec!["scripts/songarr-yandex".to_string()]);
    }

    #[test]
    fn installed_helper_runs_directly() {
        let (program, args) = helper_invocation("/usr/local/bin/songarr-yandex");
        assert_eq!(program, PathBuf::from("/usr/local/bin/songarr-yandex"));
        assert!(args.is_empty());
    }
}
