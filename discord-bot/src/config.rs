use anyhow::Context as _;

#[derive(Clone)]
pub struct Config {
    /// Discord bot token.
    pub token: String,
    /// Default Songarr base URL offered when a user runs `/link` without one.
    pub default_server: Option<String>,
    /// SQLite path for per-user account links.
    pub db_path: String,
    /// If set, register slash commands to this guild (instant) instead of
    /// globally (which can take up to an hour to propagate).
    pub test_guild: Option<u64>,
    /// Outgoing Discord Opus bitrate for music playback.
    pub voice_bitrate_kbps: u32,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let voice_bitrate_kbps = std::env::var("SONGARR_DISCORD_BITRATE_KBPS")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(256)
            .clamp(16, 512);
        Ok(Self {
            token: std::env::var("DISCORD_TOKEN").context("DISCORD_TOKEN is required")?,
            default_server: non_empty(std::env::var("SONGARR_URL").ok()),
            db_path: non_empty(std::env::var("SONGARR_DISCORD_DB").ok())
                .unwrap_or_else(|| "songarr-discord.db".to_string()),
            test_guild: std::env::var("DISCORD_TEST_GUILD")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok()),
            voice_bitrate_kbps,
        })
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
