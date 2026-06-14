pub mod catalog;
pub mod config;
pub mod db;
pub mod ingest;
pub mod jobs;
pub mod lyrics;
pub mod pending;
pub mod proxy;
pub mod recs;
pub mod resolve;
pub mod subsonic;
pub mod valbum;
pub mod vtrack;
pub mod yandex;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::routing::get;
use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::{Any, CorsLayer};

use crate::config::Config;
use crate::subsonic::Envelope;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    /// Shared client for upstream (Navidrome) calls. Redirects are NOT
    /// followed — they pass through to the Subsonic client untouched.
    pub http: reqwest::Client,
    /// Client for YouTube egress (innertube + direct media). Honors
    /// `ytdlp_proxy` — YouTube media URLs are bound to the egress IP, so
    /// this MUST share egress with yt-dlp. Follows redirects.
    pub yt_http: reqwest::Client,
    /// Bounds concurrent virtual-stream pipelines (`streaming.max_concurrent`).
    pub stream_gate: Arc<tokio::sync::Semaphore>,
    /// Bounds background resolution prefetch (never starves real plays).
    pub resolve_gate: Arc<tokio::sync::Semaphore>,
    /// Tracks ids currently being prefetch-resolved (dedup across searches).
    pub resolve_inflight: Arc<tokio::sync::Mutex<std::collections::HashSet<String>>>,
    /// Tracks artist expansion prewarms currently in flight (dedup across
    /// streams/scrobbles/discovery generation).
    pub artist_prewarm_inflight: Arc<tokio::sync::Mutex<std::collections::HashSet<String>>>,
    envelope_cache: Arc<tokio::sync::OnceCell<Envelope>>,
}

impl AppState {
    pub fn new(config: Arc<Config>, db: SqlitePool) -> anyhow::Result<Self> {
        // Fail fast on an unusable upstream URL instead of 502ing per request.
        reqwest::Url::parse(&config.navidrome.base_url)
            .map_err(|e| anyhow::anyhow!("invalid navidrome.base_url: {e}"))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let mut yt_builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
        if !config.streaming.ytdlp_proxy.is_empty() {
            yt_builder = yt_builder.proxy(reqwest::Proxy::all(&config.streaming.ytdlp_proxy)?);
        }
        let yt_http = yt_builder.build()?;
        let stream_gate = Arc::new(tokio::sync::Semaphore::new(
            config.streaming.max_concurrent.max(1) as usize,
        ));
        Ok(Self {
            config,
            db,
            http,
            yt_http,
            stream_gate,
            resolve_gate: Arc::new(tokio::sync::Semaphore::new(2)),
            resolve_inflight: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
            artist_prewarm_inflight: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            envelope_cache: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Envelope attributes for synthesized responses, mirrored from a real
    /// Navidrome ping (admin creds) and cached. Falls back to defaults —
    /// and retries next time — if Navidrome is unreachable.
    pub async fn envelope(&self) -> Envelope {
        if let Some(envelope) = self.envelope_cache.get() {
            return envelope.clone();
        }
        match self.fetch_envelope().await {
            Ok(envelope) => {
                let _ = self.envelope_cache.set(envelope.clone());
                envelope
            }
            Err(error) => {
                tracing::debug!(%error, "envelope ping failed; using defaults");
                Envelope::default()
            }
        }
    }

    async fn fetch_envelope(&self) -> anyhow::Result<Envelope> {
        let url = format!(
            "{}/rest/ping?{}&f=json",
            self.config.navidrome.base_url.trim_end_matches('/'),
            subsonic::auth::admin_auth_query(&self.config.navidrome)
        );
        let value: serde_json::Value = self
            .http
            .get(url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .json()
            .await?;
        let response = &value["subsonic-response"];
        anyhow::ensure!(response.is_object(), "not a subsonic response");
        Ok(Envelope {
            version: response["version"].as_str().unwrap_or("1.16.1").into(),
            server_type: response["type"].as_str().unwrap_or("navidrome").into(),
            server_version: response["serverVersion"].as_str().unwrap_or("").into(),
            open_subsonic: response["openSubsonic"].as_bool().unwrap_or(true),
        })
    }

    pub fn artwork_cache_dir(&self) -> PathBuf {
        self.config
            .server
            .db_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("artwork")
    }
}

/// Build the full application router: explicit interceptions plus the
/// transparent Navidrome passthrough for everything else. Subsonic endpoints
/// are reachable both bare and with the legacy `.view` suffix.
pub fn build_app(state: AppState) -> Router {
    use axum::routing::any;
    macro_rules! intercept {
        ($router:expr, $endpoint:literal, $handler:expr) => {
            $router
                .route(concat!("/rest/", $endpoint), any($handler))
                .route(concat!("/rest/", $endpoint, ".view"), any($handler))
        };
    }
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/wave", get(proxy::wave::index))
        .route("/wave/", get(proxy::wave::index))
        .route("/wave/spike", get(proxy::wave::spike))
        .route("/wave/api/next", get(proxy::wave::next_handler))
        .route(
            "/wave/api/feedback",
            axum::routing::post(proxy::wave::feedback_handler),
        )
        .route("/wave/{*path}", get(proxy::wave::asset));
    let router = intercept!(router, "search2", proxy::search::search2_handler);
    let router = intercept!(router, "search3", proxy::search::search3_handler);
    let router = intercept!(router, "getSong", proxy::song::handler);
    let router = intercept!(router, "getArtist", proxy::artist::handler);
    let router = intercept!(router, "getAlbum", proxy::album::handler);
    let router = intercept!(router, "getCoverArt", proxy::coverart::handler);
    let router = intercept!(router, "stream", proxy::stream::handler);
    let router = intercept!(router, "download", proxy::stream::handler);
    let router = intercept!(
        router,
        "getSimilarSongs",
        proxy::similar::similar_songs_handler
    );
    let router = intercept!(
        router,
        "getSimilarSongs2",
        proxy::similar::similar_songs2_handler
    );
    let router = intercept!(router, "getTopSongs", proxy::similar::top_songs_handler);
    let router = intercept!(
        router,
        "getPlaylists",
        proxy::playlists::get_playlists_handler
    );
    let router = intercept!(
        router,
        "getPlaylist",
        proxy::playlists::get_playlist_handler
    );
    let router = intercept!(router, "scrobble", proxy::scrobble::scrobble_handler);
    let router = intercept!(router, "star", proxy::scrobble::star_handler);
    let router = intercept!(router, "unstar", proxy::scrobble::unstar_handler);
    let router = intercept!(router, "getLyricsBySongId", proxy::lyrics::handler);
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    router
        .fallback(proxy::passthrough::handler)
        .with_state(state)
        .layer(cors)
}

async fn healthz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => (StatusCode::OK, "ok"),
        Err(error) => {
            tracing::error!(%error, "healthz database check failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "database unavailable")
        }
    }
}
