pub mod deezer;
pub mod yandex;

use crate::vtrack::CatalogTrack;

/// Search the configured external catalog. Errors are the caller's problem
/// only insofar as logging — search injection must never break passthrough.
pub async fn search(
    http: &reqwest::Client,
    config: &crate::config::Config,
    query: &str,
) -> anyhow::Result<Vec<CatalogTrack>> {
    let search = &config.external_search;
    let mut tracks = Vec::new();
    match search.provider {
        crate::config::Provider::Deezer | crate::config::Provider::Ytmusic => {
            tracks.extend(
                deezer::search(http, &search.api_base_deezer, query, search.max_results).await?,
            );
        }
        crate::config::Provider::Yandex => {
            tracks.extend(yandex::search(&config.yandex, query, search.max_results).await?);
        }
        crate::config::Provider::Both => {
            tracks.extend(
                deezer::search(http, &search.api_base_deezer, query, search.max_results).await?,
            );
            match yandex::search(&config.yandex, query, search.max_results).await {
                Ok(results) => tracks.extend(results),
                Err(error) => tracing::debug!(%error, "Yandex catalog source abstained"),
            }
        }
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    #[test]
    fn yandex_provider_name_is_stable() {
        assert_eq!(crate::yandex::PROVIDER, "yandex");
    }
}
