use crate::playback::{current_call, enqueue, install_wave_refiller, join_user_channel};
use crate::songarr::{make_link, require_server, SongarrClient};
use crate::store::{self, Link};
use crate::{Context, Data, Error};

/// All slash commands, for registration.
pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![
        link(),
        unlink(),
        play(),
        wave(),
        skip(),
        pause(),
        resume(),
        stop(),
        queue(),
        nowplaying(),
    ]
}

async fn user_link(ctx: Context<'_>) -> Result<Link, Error> {
    let id = ctx.author().id.get();
    match store::get_link(&ctx.data().db, id).await? {
        Some(link) => Ok(link),
        None => Err("Сначала привяжи аккаунт Songarr командой `/link`.".into()),
    }
}

async fn reply_ephemeral(ctx: Context<'_>, content: impl Into<String>) -> Result<(), Error> {
    ctx.send(poise::CreateReply::default().ephemeral(true).content(content.into()))
        .await?;
    Ok(())
}

/// Link your Songarr account so the bot can play your library and wave.
#[poise::command(slash_command)]
async fn link(
    ctx: Context<'_>,
    #[description = "Имя пользователя Songarr"] username: String,
    #[description = "Пароль"] password: String,
    #[description = "URL сервера (если не задан по умолчанию)"] server: Option<String>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let default = ctx.data().config.default_server.as_deref();
    let server_url = require_server(server.as_deref(), default)?;
    let new_link = make_link(server_url, &username, &password);

    let client = SongarrClient::new(&ctx.data().http, &new_link);
    if let Err(error) = client.ping().await {
        reply_ephemeral(ctx, format!("Не получилось войти: {error}")).await?;
        return Ok(());
    }
    store::set_link(&ctx.data().db, ctx.author().id.get(), &new_link).await?;
    reply_ephemeral(ctx, format!("✅ Аккаунт **{username}** привязан.")).await?;
    Ok(())
}

/// Unlink your Songarr account from this bot.
#[poise::command(slash_command)]
async fn unlink(ctx: Context<'_>) -> Result<(), Error> {
    let removed = store::delete_link(&ctx.data().db, ctx.author().id.get()).await?;
    reply_ephemeral(
        ctx,
        if removed { "Аккаунт отвязан." } else { "Привязки и не было." },
    )
    .await
}

/// Search your library and play a track in your voice channel.
#[poise::command(slash_command, guild_only)]
async fn play(
    ctx: Context<'_>,
    #[description = "Песня, артист или альбом"] query: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let link = user_link(ctx).await?;
    let http = ctx.data().http.clone();

    let client = SongarrClient::new(&http, &link);
    let Some(track) = client.search_song(&query).await? else {
        ctx.say("Ничего не нашёл.").await?;
        return Ok(());
    };
    let url = client.stream_url(&track);
    let label = track.label();

    let (_, _, call) = join_user_channel(ctx).await?;
    enqueue(&call, &ctx.data().stream_http, url, label.clone(), &ctx.data().labels).await;
    ctx.say(format!("▶️ В очередь: **{label}**")).await?;
    Ok(())
}

/// Start your endless personalised Wave in your voice channel.
#[poise::command(slash_command, guild_only)]
async fn wave(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let link = user_link(ctx).await?;
    let http = ctx.data().http.clone();

    let client = SongarrClient::new(&http, &link);
    let tracks = client.wave_next(None, 12).await?;
    if tracks.is_empty() {
        ctx.say("Волна пока пустая — послушай что-нибудь, чтобы её настроить.").await?;
        return Ok(());
    }

    let (manager, guild_id, call) = join_user_channel(ctx).await?;
    for track in &tracks {
        enqueue(&call, &ctx.data().stream_http, client.stream_url(track), track.label(), &ctx.data().labels)
            .await;
    }
    install_wave_refiller(
        &call,
        manager,
        guild_id,
        http.clone(),
        ctx.data().stream_http.clone(),
        link.clone(),
        ctx.data().labels.clone(),
    )
    .await;
    ctx.say(format!("🌊 Твоя волна пошла — {} треков в очереди, дальше бесконечно.", tracks.len()))
        .await?;
    Ok(())
}

/// Skip the current track.
#[poise::command(slash_command, guild_only)]
async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let call = current_call(ctx).await?;
    let _ = call.lock().await.queue().skip();
    ctx.say("⏭️ Пропустил.").await?;
    Ok(())
}

/// Pause playback.
#[poise::command(slash_command, guild_only)]
async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    let call = current_call(ctx).await?;
    let _ = call.lock().await.queue().pause();
    ctx.say("⏸️ Пауза.").await?;
    Ok(())
}

/// Resume playback.
#[poise::command(slash_command, guild_only)]
async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    let call = current_call(ctx).await?;
    let _ = call.lock().await.queue().resume();
    ctx.say("▶️ Продолжаю.").await?;
    Ok(())
}

/// Stop and leave the voice channel.
#[poise::command(slash_command, guild_only)]
async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Только на сервере.")?;
    if let Some(call) = current_call(ctx).await.ok() {
        call.lock().await.queue().stop();
    }
    if let Some(manager) = songbird::get(ctx.serenity_context()).await {
        let _ = manager.remove(guild_id).await;
    }
    ctx.say("⏹️ Остановил и вышел.").await?;
    Ok(())
}

/// Show the upcoming queue.
#[poise::command(slash_command, guild_only)]
async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let call = current_call(ctx).await?;
    let handles = call.lock().await.queue().current_queue();
    if handles.is_empty() {
        ctx.say("Очередь пуста.").await?;
        return Ok(());
    }
    let labels = ctx.data().labels.lock().await;
    let mut lines = Vec::new();
    for (i, handle) in handles.iter().take(15).enumerate() {
        let label = labels
            .get(&handle.uuid().to_string())
            .cloned()
            .unwrap_or_else(|| "Трек".to_string());
        let marker = if i == 0 { "▶️" } else { &format!("{}.", i) };
        lines.push(format!("{marker} {label}"));
    }
    let more = handles.len().saturating_sub(15);
    if more > 0 {
        lines.push(format!("…и ещё {more}"));
    }
    ctx.say(lines.join("\n")).await?;
    Ok(())
}

/// Show what's playing now.
#[poise::command(slash_command, guild_only)]
async fn nowplaying(ctx: Context<'_>) -> Result<(), Error> {
    let call = current_call(ctx).await?;
    let current = call.lock().await.queue().current();
    match current {
        Some(handle) => {
            let label = ctx
                .data()
                .labels
                .lock()
                .await
                .get(&handle.uuid().to_string())
                .cloned()
                .unwrap_or_else(|| "Трек".to_string());
            ctx.say(format!("🎶 Сейчас играет: **{label}**")).await?;
        }
        None => {
            ctx.say("Сейчас ничего не играет.").await?;
        }
    }
    Ok(())
}
