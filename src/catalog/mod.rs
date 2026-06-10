pub mod deezer;

use crate::vtrack::CatalogTrack;

/// Search the configured external catalog. Errors are the caller's problem
/// only insofar as logging — search injection must never break passthrough.
pub async fn search(
    http: &reqwest::Client,
    config: &crate::config::ExternalSearch,
    query: &str,
) -> anyhow::Result<Vec<CatalogTrack>> {
    match config.provider {
        crate::config::Provider::Deezer => {
            deezer::search(http, &config.api_base_deezer, query, config.max_results).await
        }
        // ytmusic arrives with M3's yt-dlp plumbing; deezer covers v1.
        crate::config::Provider::Ytmusic | crate::config::Provider::Both => {
            deezer::search(http, &config.api_base_deezer, query, config.max_results).await
        }
    }
}
