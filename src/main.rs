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

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("yandex")
        && args.get(2).map(String::as_str) == Some("login")
    {
        return yandex_login().await;
    }

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

async fn yandex_login() -> anyhow::Result<()> {
    let helper = std::env::var("SONGARR_YANDEX_HELPER_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            let local = PathBuf::from("scripts/songarr-yandex");
            if local.exists() {
                local.to_string_lossy().into_owned()
            } else {
                "/usr/local/bin/songarr-yandex".into()
            }
        });
    let helper_path = PathBuf::from(&helper);
    let repo_helper = helper_path.ends_with("scripts/songarr-yandex");
    let mut command = if repo_helper {
        let python = std::env::var("VIRTUAL_ENV")
            .map(|venv| PathBuf::from(venv).join("bin/python"))
            .ok()
            .filter(|path| path.exists())
            .unwrap_or_else(|| PathBuf::from("python3"));
        eprintln!(
            "Launching Yandex helper: {} {} login",
            python.display(),
            helper_path.display()
        );
        let mut command = tokio::process::Command::new(python);
        command.arg(&helper_path);
        command
    } else {
        eprintln!("Launching Yandex helper: {} login", helper_path.display());
        tokio::process::Command::new(&helper_path)
    };
    let status = command
        .arg("login")
        .env("PYTHONUNBUFFERED", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await?;
    anyhow::ensure!(status.success(), "Yandex login helper exited with {status}");
    Ok(())
}
