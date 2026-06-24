//! JSON bridge to the Python VK Music helper.
//!
//! yt-dlp has no VK *audio* extractor, so pasted `vk.com/audio…` links are
//! resolved here instead — the same pattern as [`crate::yandex`]: Rust sends one
//! JSON object on stdin and reads JSON on stdout from `scripts/songarr-vk`,
//! which uses the unofficial VK audio API (`audio.getById`) under a Kate-Mobile
//! token. VK serves audio as encrypted HLS, so the returned `url` is handed to
//! ffmpeg by the stream pipeline rather than fetched as a plain body.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::config::Vk as VkConfig;

pub const PROVIDER: &str = "vk";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VkTrack {
    #[serde(alias = "track_id", alias = "trackId", alias = "id")]
    pub track_id: String,
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VkMedia {
    /// Direct media URL (usually an HLS `.m3u8`); consumed by ffmpeg.
    pub url: String,
    #[serde(default)]
    pub codec: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackRequest<'a> {
    access_token: &'a str,
    track_id: &'a str,
}

pub fn available(config: &VkConfig) -> bool {
    config.enabled && !config.access_token.trim().is_empty()
}

/// Metadata for a VK audio id (`<owner>_<id>`), for the virtual track.
pub async fn track_meta(config: &VkConfig, track_id: &str) -> anyhow::Result<VkTrack> {
    anyhow::ensure!(available(config), "VK disabled or token missing");
    run_helper(
        config,
        "track",
        &TrackRequest {
            access_token: &config.access_token,
            track_id,
        },
    )
    .await
}

/// The playable media URL for a VK audio id (resolved at play time).
pub async fn resolve(config: &VkConfig, track_id: &str) -> anyhow::Result<VkMedia> {
    anyhow::ensure!(available(config), "VK disabled or token missing");
    run_helper(
        config,
        "download",
        &TrackRequest {
            access_token: &config.access_token,
            track_id,
        },
    )
    .await
}

async fn run_helper<T: DeserializeOwned, P: Serialize>(
    config: &VkConfig,
    command: &str,
    payload: &P,
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
        .map_err(|error| anyhow::anyhow!("starting VK helper failed: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&input).await?;
    }

    let timeout = Duration::from_secs(config.api_timeout_secs.max(1));
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("VK helper timed out after {}s", timeout.as_secs()))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("VK helper {command} failed: {}", stderr.trim());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("invalid VK helper JSON: {error}"))
}

fn helper_invocation(helper_path: &str) -> (PathBuf, Vec<String>) {
    let helper = PathBuf::from(helper_path);
    let is_source_helper = helper.ends_with("scripts/songarr-vk");
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
        let track: VkTrack = serde_json::from_value(serde_json::json!({
            "trackId": "-2001262717_136262717",
            "artist": "Молчат Дома",
            "title": "Судно",
            "album": "Этажи",
            "durationMs": 200000
        }))
        .unwrap();
        assert_eq!(track.track_id, "-2001262717_136262717");
        assert_eq!(track.duration_ms, Some(200000));
    }

    #[test]
    fn unavailable_without_token() {
        let config = VkConfig {
            enabled: true,
            ..VkConfig::default()
        };
        assert!(!available(&config));
    }

    #[test]
    fn source_helper_runs_through_python() {
        let (program, args) = helper_invocation("scripts/songarr-vk");
        assert!(program.ends_with("python") || program.ends_with("python3"));
        assert_eq!(args, vec!["scripts/songarr-vk".to_string()]);
    }

    #[test]
    fn installed_helper_runs_directly() {
        let (program, args) = helper_invocation("/usr/local/bin/songarr-vk");
        assert_eq!(program, PathBuf::from("/usr/local/bin/songarr-vk"));
        assert!(args.is_empty());
    }
}
