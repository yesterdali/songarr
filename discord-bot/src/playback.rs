use std::collections::HashMap;
use std::sync::Arc;

use poise::serenity_prelude::GuildId;
use songbird::input::HttpRequest;
use songbird::{Call, Event, EventContext, EventHandler as VoiceEventHandler, Songbird, TrackEvent};
use tokio::sync::Mutex;

use crate::songarr::{SongarrClient, Track};
use crate::store::Link;
use crate::{Context, Error};

/// Shared map of songbird track UUID → human label, so `/queue` and
/// `/nowplaying` can show titles (songbird itself only tracks opaque handles).
pub type LabelMap = Arc<Mutex<HashMap<String, String>>>;

/// Join the voice channel the invoking user is currently in.
pub async fn join_user_channel(
    ctx: Context<'_>,
) -> Result<(Arc<Songbird>, GuildId, Arc<Mutex<Call>>), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("Эту команду можно использовать только на сервере.")?;
    let channel_id = {
        let guild = ctx.guild().ok_or("Не удалось получить данные сервера.")?;
        guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|state| state.channel_id)
    };
    let channel_id = channel_id.ok_or("Сначала зайди в голосовой канал.")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Голосовой движок не инициализирован.")?
        .clone();
    let call = manager.join(guild_id, channel_id).await?;
    Ok((manager, guild_id, call))
}

/// The active voice call for the invoking guild, if any.
pub async fn current_call(ctx: Context<'_>) -> Result<Arc<Mutex<Call>>, Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("Эту команду можно использовать только на сервере.")?;
    let manager = songbird::get(ctx.serenity_context())
        .await
        .ok_or("Голосовой движок не инициализирован.")?;
    manager
        .get(guild_id)
        .ok_or_else(|| "Сейчас ничего не играет.".into())
}

/// Enqueue one track's stream into the voice call and record its label.
pub async fn enqueue(
    call: &Arc<Mutex<Call>>,
    http: &reqwest::Client,
    url: String,
    label: String,
    labels: &LabelMap,
) {
    let input = HttpRequest::new(http.clone(), url);
    let handle = {
        let mut handler = call.lock().await;
        handler.enqueue_input(input.into()).await
    };
    labels.lock().await.insert(handle.uuid().to_string(), label);
}

/// Tops the queue back up with fresh Wave recommendations whenever it runs low,
/// making `/wave` endless.
pub struct WaveRefiller {
    pub manager: Arc<Songbird>,
    pub guild_id: GuildId,
    /// API client (wave_next); has a total timeout.
    pub http: reqwest::Client,
    /// Streaming client (audio fetch); no total timeout.
    pub stream_http: reqwest::Client,
    pub link: Link,
    pub labels: LabelMap,
}

#[async_trait::async_trait]
impl VoiceEventHandler for WaveRefiller {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let call_lock = self.manager.get(self.guild_id)?;
        let remaining = call_lock.lock().await.queue().len();
        if remaining > 2 {
            return None;
        }
        let client = SongarrClient::new(&self.http, &self.link);
        let tracks: Vec<Track> = client.wave_next(None, 12).await.unwrap_or_default();
        for track in tracks {
            let input = HttpRequest::new(self.stream_http.clone(), client.stream_url(&track));
            let handle = {
                let mut handler = call_lock.lock().await;
                handler.enqueue_input(input.into()).await
            };
            self.labels
                .lock()
                .await
                .insert(handle.uuid().to_string(), track.label());
        }
        None
    }
}

/// Register the endless-wave refiller on a call.
pub async fn install_wave_refiller(
    call: &Arc<Mutex<Call>>,
    manager: Arc<Songbird>,
    guild_id: GuildId,
    http: reqwest::Client,
    stream_http: reqwest::Client,
    link: Link,
    labels: LabelMap,
) {
    let mut handler = call.lock().await;
    handler.add_global_event(
        Event::Track(TrackEvent::End),
        WaveRefiller { manager, guild_id, http, stream_http, link, labels },
    );
}
