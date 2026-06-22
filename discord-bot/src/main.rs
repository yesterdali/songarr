mod commands;
mod config;
mod playback;
mod songarr;
mod store;

use std::collections::HashMap;
use std::sync::Arc;

use poise::serenity_prelude as serenity;
use songbird::SerenityInit;
use tokio::sync::Mutex;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Shared bot state available to every command.
pub struct Data {
    pub config: config::Config,
    pub db: sqlx::SqlitePool,
    pub http: reqwest::Client,
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
    let http = reqwest::Client::builder()
        .user_agent("songarr-discord/0.1")
        .build()?;

    let data = Data {
        config: config.clone(),
        db,
        http,
        labels: Arc::new(Mutex::new(HashMap::new())),
    };

    let options = poise::FrameworkOptions {
        commands: commands::all(),
        ..Default::default()
    };

    let test_guild = config.test_guild;
    let framework = poise::Framework::builder()
        .options(options)
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                match test_guild {
                    Some(id) => {
                        poise::builtins::register_in_guild(
                            ctx,
                            &framework.options().commands,
                            serenity::GuildId::new(id),
                        )
                        .await?;
                    }
                    None => {
                        poise::builtins::register_globally(ctx, &framework.options().commands)
                            .await?;
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
