// TODO(M1+): remove once the proxy/catalog/stream modules consume these fields.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

/// Top-level configuration, mirroring `config.example.toml`.
/// Every section and field has a default so a minimal config file works.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: Server,
    #[serde(default)]
    pub navidrome: Navidrome,
    #[serde(default)]
    pub library: Library,
    #[serde(default)]
    pub external_search: ExternalSearch,
    #[serde(default)]
    pub streaming: Streaming,
    #[serde(default)]
    pub ingest: Ingest,
    #[serde(default)]
    pub upgrade: Upgrade,
    #[serde(default)]
    pub users: Users,
    #[serde(default)]
    pub recommendations: Recommendations,
    #[serde(default)]
    pub artist_expansion: ArtistExpansion,
    #[serde(default)]
    pub lyrics: Lyrics,
    #[serde(default)]
    pub yandex: Yandex,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Yandex {
    pub enabled: bool,
    /// Path to the JSON helper. The Docker image installs this here; tests
    /// override it with a mock helper.
    pub helper_path: String,
    /// Server-side account tokens. Leave empty to keep the provider inactive.
    pub access_token: String,
    pub refresh_token: String,
    pub use_for_wave: bool,
    pub use_for_search: bool,
    pub use_for_import: bool,
    pub api_timeout_secs: u64,
}

impl Default for Yandex {
    fn default() -> Self {
        Self {
            enabled: false,
            helper_path: "/usr/local/bin/songarr-yandex".into(),
            access_token: String::new(),
            refresh_token: String::new(),
            use_for_wave: true,
            use_for_search: true,
            use_for_import: true,
            api_timeout_secs: 10,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Lyrics {
    /// getLyricsBySongId fallback lookup for tracks without Navidrome lyrics.
    pub enabled: bool,
    /// Override for tests / API proxies; normally leave at LRCLIB.
    pub lrclib_api_base: String,
}

impl Default for Lyrics {
    fn default() -> Self {
        Self {
            enabled: true,
            lrclib_api_base: "https://lrclib.net".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ArtistExpansion {
    /// getArtist/getAlbum enrichment with provider albums. Off leaves both
    /// endpoints as plain Navidrome passthrough.
    pub enabled: bool,
    pub max_albums: u32,
    pub max_tracks_per_album: u32,
    pub cache_ttl_hours: u32,
    pub include_singles: bool,
    pub include_top_tracks_album: bool,
    /// 0-100 resolver confidence required before querying an artist catalog.
    pub min_artist_match_score: u32,
}

impl Default for ArtistExpansion {
    fn default() -> Self {
        Self {
            enabled: true,
            max_albums: 12,
            max_tracks_per_album: 30,
            cache_ttl_hours: 168,
            include_singles: true,
            include_top_tracks_album: true,
            min_artist_match_score: 70,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Recommendations {
    /// getSimilarSongs/getSimilarSongs2/getTopSongs interception. Off →
    /// those endpoints pass through to Navidrome untouched.
    pub enabled: bool,
    /// Cap on injected recommendation entries per response (the client's
    /// own `count` param can only lower it).
    pub max_results: u32,
    pub shown_cooldown_days: u32,
    pub cache_ttl_hours: u32,
    pub weight_ytm: f32,
    pub weight_deezer: f32,
    pub weight_lastfm: f32,
    pub weight_yandex: f32,
    pub lastfm_api_key: String,
    /// Override for tests; rarely set by hand. YTM song-radio lives on
    /// music.youtube.com, not www.
    pub ytm_api_base: String,
    /// Override for tests; rarely set by hand.
    pub lastfm_api_base: String,
}

impl Default for Recommendations {
    fn default() -> Self {
        Self {
            enabled: true,
            max_results: 20,
            shown_cooldown_days: 7,
            cache_ttl_hours: 72,
            weight_ytm: 1.0,
            weight_deezer: 0.6,
            weight_lastfm: 0.8,
            weight_yandex: 1.2,
            lastfm_api_key: String::new(),
            ytm_api_base: "https://music.youtube.com".into(),
            lastfm_api_base: "https://ws.audioscrobbler.com/2.0".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Server {
    pub bind: String,
    pub db_path: PathBuf,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:4534".into(),
            db_path: "/config/songarr.db".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Navidrome {
    pub base_url: String,
    /// Admin creds are used ONLY for proxy-originated calls (scan trigger,
    /// post-import verification), never for serving user requests.
    pub admin_user: String,
    pub admin_password: String,
}

impl Default for Navidrome {
    fn default() -> Self {
        Self {
            base_url: "http://navidrome:4533".into(),
            admin_user: "admin".into(),
            admin_password: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Library {
    pub music_dir: PathBuf,
    pub ingest_subdir: String,
    pub staging_dir: PathBuf,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            music_dir: "/music".into(),
            ingest_subdir: "_songarr".into(),
            staging_dir: "/staging".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ExternalSearch {
    pub enabled: bool,
    pub provider: Provider,
    pub max_results: u32,
    pub min_query_len: u32,
    /// Override for tests / API proxies; rarely set by hand.
    pub api_base_deezer: String,
}

impl Default for ExternalSearch {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: Provider::Deezer,
            max_results: 8,
            min_query_len: 3,
            api_base_deezer: "https://api.deezer.com".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Deezer,
    Ytmusic,
    Yandex,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Streaming {
    pub ytdlp_path: String,
    /// e.g. "http://gluetun:8888" — passed to `yt-dlp --proxy` AND used for
    /// direct innertube/media fetches (YouTube URLs are egress-IP-bound).
    pub ytdlp_proxy: String,
    /// Native innertube fast path (~1s to audio); any failure silently
    /// falls back to the yt-dlp pipe. Off by default: googlevideo serves
    /// only the first ~1 MiB of a stream without a PO token, so on token-
    /// required egress IPs (datacenter/proxy) this path validates, 403s past
    /// the window, and falls back on every play — wasting a few requests for
    /// no gain. Enable only where the egress IP isn't token-gated.
    pub innertube: bool,
    /// Override for tests; rarely set by hand.
    pub innertube_api_base: String,
    pub format: StreamFormat,
    pub bitrate_kbps: u32,
    pub max_concurrent: u32,
    pub timeout_first_byte_secs: u64,
}

impl Default for Streaming {
    fn default() -> Self {
        Self {
            ytdlp_path: "yt-dlp".into(),
            ytdlp_proxy: String::new(),
            innertube: false,
            innertube_api_base: "https://www.youtube.com".into(),
            format: StreamFormat::Opus,
            bitrate_kbps: 160,
            max_concurrent: 3,
            timeout_first_byte_secs: 12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamFormat {
    Opus,
    Mp3,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ingest {
    pub auto_import: bool,
    pub min_score_for_import: u32,
    /// Rolling playlist of imports; empty string disables.
    pub write_playlist: String,
    /// Worker poll interval; mostly a test knob.
    pub poll_secs: u64,
}

impl Default for Ingest {
    fn default() -> Self {
        Self {
            auto_import: true,
            min_score_for_import: 80,
            write_playlist: "Songarr Played.m3u".into(),
            poll_secs: 5,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Upgrade {
    pub mode: UpgradeMode,
    pub soulsync_url: String,
}

impl Default for Upgrade {
    fn default() -> Self {
        Self {
            mode: UpgradeMode::None,
            soulsync_url: "http://soulsync:8888".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeMode {
    None,
    SoulsyncWishlist,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Users {
    pub default_can_trigger_downloads: bool,
    /// Usernames who get passthrough-only behavior (no virtual results).
    pub deny: Vec<String>,
}

impl Default for Users {
    fn default() -> Self {
        Self {
            default_can_trigger_downloads: true,
            deny: Vec::new(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        let mut config: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing config file {}", path.display()))?;
        config.apply_env_overrides();
        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(value) = std::env::var("SONGARR_YANDEX_ENABLED") {
            self.yandex.enabled = matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Ok(value) = std::env::var("SONGARR_YANDEX_HELPER_PATH") {
            if !value.trim().is_empty() {
                self.yandex.helper_path = value;
            }
        }
        if let Ok(value) = std::env::var("SONGARR_YANDEX_ACCESS_TOKEN") {
            self.yandex.access_token = value;
        }
        if let Ok(value) = std::env::var("SONGARR_YANDEX_REFRESH_TOKEN") {
            self.yandex.refresh_token = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.server.bind, "0.0.0.0:4534");
        assert_eq!(config.external_search.max_results, 8);
        assert_eq!(config.streaming.format, StreamFormat::Opus);
        assert_eq!(config.upgrade.mode, UpgradeMode::None);
        assert_eq!(config.recommendations.cache_ttl_hours, 72);
        assert_eq!(config.recommendations.weight_yandex, 1.2);
        assert!(config.lyrics.enabled);
        assert_eq!(config.lyrics.lrclib_api_base, "https://lrclib.net");
        assert!(!config.yandex.enabled);
        assert!(config.yandex.use_for_import);
        assert!(config.users.deny.is_empty());
    }

    #[test]
    fn example_config_parses() {
        let raw = include_str!("../config.example.toml");
        let config: Config = toml::from_str(raw).unwrap();
        assert_eq!(config.navidrome.base_url, "http://navidrome:4533");
        assert_eq!(config.ingest.min_score_for_import, 80);
        assert_eq!(config.streaming.timeout_first_byte_secs, 12);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = toml::from_str::<Config>("[server]\nbnd = \"x\"\n").unwrap_err();
        assert!(err.to_string().contains("bnd"));
    }
}
