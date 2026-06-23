mod commands;
mod config;
mod playback;
mod songarr;
mod store;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use songbird::SerenityInit;
use tokio::sync::Mutex;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Shared bot state available to every command.
pub struct Data {
    pub config: config::Config,
    pub db: sqlx::SqlitePool,
    /// Short API calls (ping/search/wave): carries a total timeout to fail fast.
    pub http: reqwest::Client,
    /// Audio streaming: NO total timeout (a track runs far longer than any such
    /// deadline would allow), only connect + read-inactivity timeouts.
    pub stream_http: reqwest::Client,
    pub labels: playback::LabelMap,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "songarr_discord=info,warn".into()),
        )
        .init();

    let config = config::Config::from_env()?;
    let db = store::init(&config.db_path).await?;
    // Bounded timeouts so an unreachable/wrong SONGARR_URL fails fast with a
    // clear error instead of leaving a slash command "thinking" forever.
    let http = reqwest::Client::builder()
        .user_agent("songarr-discord/0.1")
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()?;
    // Streaming client: a track's body is read for its whole duration, so a
    // total `.timeout()` would abort playback mid-song (~20s in). Use only a
    // connect timeout plus a read-inactivity timeout, so a genuinely stalled
    // connection still errors out and lets the next track start.
    let stream_http = reqwest::Client::builder()
        .user_agent("songarr-discord/0.1")
        .connect_timeout(Duration::from_secs(8))
        .read_timeout(Duration::from_secs(30))
        .build()?;

    let data = Data {
        config: config.clone(),
        db,
        http,
        stream_http,
        labels: Arc::new(Mutex::new(HashMap::new())),
    };

    let options = poise::FrameworkOptions {
        commands: commands::all(),
        on_error: |error| Box::pin(on_error(error)),
        ..Default::default()
    };

    let test_guild = config.test_guild;
    let framework = poise::Framework::builder()
        .options(options)
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                let count = framework.options().commands.len();
                match test_guild {
                    Some(id) => {
                        poise::builtins::register_in_guild(
                            ctx,
                            &framework.options().commands,
                            serenity::GuildId::new(id),
                        )
                        .await?;
                        tracing::info!(
                            "connected as {}; registered {count} commands to guild {id} (instant)",
                            ready.user.name,
                        );
                    }
                    None => {
                        poise::builtins::register_globally(ctx, &framework.options().commands)
                            .await?;
                        tracing::warn!(
                            "connected as {}; registered {count} commands GLOBALLY — they can \
                             take up to ~1h to appear. Set DISCORD_TEST_GUILD for instant testing.",
                            ready.user.name,
                        );
                    }
                }
                Ok(data)
            })
        })
        .build();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let mut client = serenity::ClientBuilder::new(&config.token, intents)
        .framework(framework)
        .register_songbird()
        .await?;

    client.start().await?;
    Ok(())
}

/// Surface command failures to the user (poise's default only logs them), so a
/// bad URL / wrong password / unreachable server shows up in Discord instead of
/// a silent "nothing happened".
async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx, .. } => {
            tracing::error!(command = ctx.command().name, %error, "command failed");
            let _ = ctx
                .send(
                    poise::CreateReply::default()
                        .ephemeral(true)
                        .content(format!("⚠️ {error}")),
                )
                .await;
        }
        other => {
            if let Err(e) = poise::builtins::on_error(other).await {
                tracing::error!(%e, "error while handling a framework error");
            }
        }
    }
}
