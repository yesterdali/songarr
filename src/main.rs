use std::path::PathBuf;
use std::sync::Arc;

use songarr_proxy::{build_app, config::Config, db, AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config_path: PathBuf = std::env::var_os("SONGARR_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/config/songarr.toml"));
    let config = Arc::new(Config::load(&config_path)?);
    tracing::info!(config = %config_path.display(), "configuration loaded");

    let db = db::init(&config.server.db_path).await?;
    tracing::info!(db = %config.server.db_path.display(), "database ready");

    let bind = config.server.bind.clone();
    let upstream = config.navidrome.base_url.clone();
    let state = AppState::new(config, db)?;
    tokio::spawn(songarr_proxy::ingest::worker(state.clone()));
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, %upstream, "songarr-proxy listening");
    axum::serve(listener, app).await?;
    Ok(())
}
