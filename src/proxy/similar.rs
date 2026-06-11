//! R1 recommendations: getSimilarSongs/getSimilarSongs2/getTopSongs.
//!
//! The implementation follows search injection's failure policy: provider
//! trouble degrades to Navidrome's own response for real ids. Virtual seeds
//! have no Navidrome equivalent, so provider failure returns an empty OK list.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::recs::merge::SourceCandidates;
use crate::recs::RecCandidate;
use crate::subsonic::types::{Payload, SongEntry};
use crate::subsonic::{auth, Format};
use crate::vtrack::{self, CatalogTrack, VirtualTrack};
use crate::AppState;

use super::{passthrough, search};

const YTM_PROVIDER: &str = "ytmusic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecKind {
    SimilarSongs,
    SimilarSongs2,
    TopSongs,
}

impl RecKind {
    fn result_key(self) -> &'static str {
        match self {
            RecKind::SimilarSongs => "similarSongs",
            RecKind::SimilarSongs2 => "similarSongs2",
            RecKind::TopSongs => "topSongs",
        }
    }
}

/// The minimum a seed needs to fan out to providers: who/what to look up, and
/// the provider identity that lets a ytmusic seed skip the YTM song search.
#[derive(Debug, Clone)]
pub(crate) struct SeedTrack {
    pub artist: String,
    pub title: String,
    pub provider: String,
    pub provider_track_id: String,
}

impl From<VirtualTrack> for SeedTrack {
    fn from(track: VirtualTrack) -> Self {
        Self {
            artist: track.artist,
            title: track.title,
            provider: track.provider,
            provider_track_id: track.provider_track_id,
        }
    }
}

pub async fn similar_songs_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, RecKind::SimilarSongs).await
}

pub async fn similar_songs2_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, RecKind::SimilarSongs2).await
}

pub async fn top_songs_handler(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, RecKind::TopSongs).await
}

async fn handle(state: AppState, req: Request, kind: RecKind) -> Response {
    let (format, username, id, artist, count) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            Format::from_query_value(params.get("f").map(|v| v.as_ref())),
            params.get("u").map(|v| v.to_string()).unwrap_or_default(),
            params.get("id").map(|v| v.to_string()).unwrap_or_default(),
            params
                .get("artist")
                .map(|v| v.trim().to_string())
                .unwrap_or_default(),
            requested_count(
                params.get("count").map(|v| v.as_ref()),
                state.config.recommendations.max_results,
            ),
        )
    };

    let Some(format) = format else {
        return passthrough::handler(State(state), req).await;
    };
    if !state.config.recommendations.enabled {
        return passthrough::handler(State(state), req).await;
    }

    match kind {
        RecKind::SimilarSongs | RecKind::SimilarSongs2 if vtrack::is_virtual_id(&id) => {
            handle_virtual_seed(state, &username, &id, kind, format, count).await
        }
        RecKind::SimilarSongs | RecKind::SimilarSongs2 => {
            handle_real_seed(state, req, &username, &id, kind, format, count).await
        }
        RecKind::TopSongs => {
            handle_top_songs(state, req, &username, &artist, kind, format, count).await
        }
    }
}

async fn handle_virtual_seed(
    state: AppState,
    username: &str,
    id: &str,
    kind: RecKind,
    format: Format,
    count: usize,
) -> Response {
    let seed = match vtrack::get(&state.db, id).await {
        Ok(Some(track)) => SeedTrack::from(track),
        Ok(None) => {
            return state
                .envelope()
                .await
                .render_ok(format, Some(songs_payload(kind.result_key(), Vec::new())));
        }
        Err(error) => {
            tracing::error!(%error, id, "recommendation seed lookup failed");
            return state
                .envelope()
                .await
                .render_error(format, 0, "internal error");
        }
    };

    let entries = match recommended_for_seed(&state, username, &seed, count, &[]).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, id, "recommendations failed for virtual seed");
            Vec::new()
        }
    };
    state
        .envelope()
        .await
        .render_ok(format, Some(songs_payload(kind.result_key(), entries)))
}

async fn handle_real_seed(
    state: AppState,
    req: Request,
    username: &str,
    id: &str,
    kind: RecKind,
    format: Format,
    count: usize,
) -> Response {
    let upstream = passthrough::fetch_upstream_identity(&state, req).await;
    let Ok((status, headers, body)) = upstream else {
        tracing::error!(id, "recommendation passthrough fetch failed");
        return (axum::http::StatusCode::BAD_GATEWAY, "upstream unavailable").into_response();
    };
    let Some(body_text) = ok_body_text(status, &body, format) else {
        return search::raw_response(status, headers, body.to_vec());
    };

    let seed = match fetch_real_seed(&state, id).await {
        Ok(seed) => seed,
        Err(error) => {
            tracing::warn!(%error, id, "real seed lookup failed; returning Navidrome recommendations");
            return search::raw_response(status, headers, body.to_vec());
        }
    };
    let existing = existing_keys(&body_text, kind.result_key(), format);
    let entries = match recommended_for_seed(&state, username, &seed, count, &existing).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, id, "recommendations failed; returning Navidrome recommendations");
            return search::raw_response(status, headers, body.to_vec());
        }
    };
    inject_entries(status, headers, &body_text, kind, format, &entries)
}

async fn handle_top_songs(
    state: AppState,
    req: Request,
    username: &str,
    artist: &str,
    kind: RecKind,
    format: Format,
    count: usize,
) -> Response {
    let upstream = passthrough::fetch_upstream_identity(&state, req).await;
    let Ok((status, headers, body)) = upstream else {
        tracing::error!(artist, "topSongs passthrough fetch failed");
        return (axum::http::StatusCode::BAD_GATEWAY, "upstream unavailable").into_response();
    };
    let Some(body_text) = ok_body_text(status, &body, format) else {
        return search::raw_response(status, headers, body.to_vec());
    };
    if artist.is_empty() || count == 0 {
        return search::raw_response(status, headers, body.to_vec());
    }

    let existing = existing_keys(&body_text, kind.result_key(), format);
    let entries = match recommended_for_artist(&state, username, artist, count, &existing).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, artist, "topSongs virtual upsert failed; returning Navidrome response");
            return search::raw_response(status, headers, body.to_vec());
        }
    };
    inject_entries(status, headers, &body_text, kind, format, &entries)
}

pub(crate) async fn recommended_for_seed(
    state: &AppState,
    username: &str,
    seed: &SeedTrack,
    count: usize,
    existing: &[search::SongKey],
) -> anyhow::Result<Vec<SongEntry>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let cfg = &state.config.recommendations;
    let fetch_limit = count.saturating_mul(2).clamp(count, 50);
    let seed_key = format!("track:{}", crate::recs::song_key(&seed.artist, &seed.title));
    let mut sources = Vec::new();

    if cfg.weight_ytm > 0.0 {
        match seed_video_id(state, seed).await {
            Ok(video_id) => match cached_source(state, "ytm_radio", &seed_key, || async {
                crate::recs::ytm::radio(
                    &state.yt_http,
                    &state.config.recommendations.ytm_api_base,
                    &video_id,
                    fetch_limit,
                )
                .await
            })
            .await
            {
                Ok(candidates) => sources.push(SourceCandidates {
                    source: "ytm",
                    weight: cfg.weight_ytm,
                    candidates,
                }),
                Err(error) => tracing::debug!(%error, "YTM radio source abstained"),
            },
            Err(error) => tracing::debug!(%error, "YTM seed resolution failed"),
        }
    }

    if cfg.weight_deezer > 0.0 {
        match cached_source(state, "deezer_similar", &seed_key, || async {
            crate::recs::deezer::similar_for_track(
                &state.http,
                &state.config.external_search.api_base_deezer,
                &seed.artist,
                &seed.title,
                fetch_limit,
            )
            .await
        })
        .await
        {
            Ok(candidates) => sources.push(SourceCandidates {
                source: "deezer",
                weight: cfg.weight_deezer,
                candidates,
            }),
            Err(error) => tracing::debug!(%error, "Deezer recommendation source abstained"),
        }
    }

    if cfg.weight_lastfm > 0.0 && !cfg.lastfm_api_key.is_empty() {
        match cached_source(state, "lastfm_similar", &seed_key, || async {
            crate::recs::lastfm::similar_for_track(
                &state.http,
                &state.config.recommendations.lastfm_api_base,
                &state.config.recommendations.lastfm_api_key,
                &seed.artist,
                &seed.title,
                fetch_limit,
            )
            .await
        })
        .await
        {
            Ok(candidates) => sources.push(SourceCandidates {
                source: "lastfm",
                weight: cfg.weight_lastfm,
                candidates,
            }),
            Err(error) => tracing::debug!(%error, "Last.fm recommendation source abstained"),
        }
    }

    let candidates = crate::recs::merge::merge_sources(sources, fetch_limit);
    upsert_candidates(state, username, candidates, count, existing).await
}

pub(crate) async fn recommended_for_artist(
    state: &AppState,
    username: &str,
    artist: &str,
    count: usize,
    existing: &[search::SongKey],
) -> anyhow::Result<Vec<SongEntry>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let cfg = &state.config.recommendations;
    let fetch_limit = count.saturating_mul(2).clamp(count, 50);
    let seed_key = format!("artist:{}", crate::recs::normalize(artist));
    let mut sources = Vec::new();

    if cfg.weight_ytm > 0.0 {
        match cached_source(state, "ytm_top", &seed_key, || async {
            crate::recs::ytm::top_songs(
                &state.yt_http,
                &state.config.recommendations.ytm_api_base,
                artist,
                fetch_limit,
            )
            .await
        })
        .await
        {
            Ok(candidates) => sources.push(SourceCandidates {
                source: "ytm",
                weight: cfg.weight_ytm,
                candidates,
            }),
            Err(error) => tracing::debug!(%error, artist, "YTM topSongs source abstained"),
        }
    }

    if cfg.weight_deezer > 0.0 {
        match cached_source(state, "deezer_top", &seed_key, || async {
            crate::recs::deezer::top_songs(
                &state.http,
                &state.config.external_search.api_base_deezer,
                artist,
                fetch_limit,
            )
            .await
        })
        .await
        {
            Ok(candidates) => sources.push(SourceCandidates {
                source: "deezer",
                weight: cfg.weight_deezer,
                candidates,
            }),
            Err(error) => tracing::debug!(%error, artist, "Deezer topSongs source abstained"),
        }
    }

    if cfg.weight_lastfm > 0.0 && !cfg.lastfm_api_key.is_empty() {
        match cached_source(state, "lastfm_top", &seed_key, || async {
            crate::recs::lastfm::top_songs(
                &state.http,
                &state.config.recommendations.lastfm_api_base,
                &state.config.recommendations.lastfm_api_key,
                artist,
                fetch_limit,
            )
            .await
        })
        .await
        {
            Ok(candidates) => sources.push(SourceCandidates {
                source: "lastfm",
                weight: cfg.weight_lastfm,
                candidates,
            }),
            Err(error) => tracing::debug!(%error, artist, "Last.fm topSongs source abstained"),
        }
    }

    let candidates = crate::recs::merge::merge_sources(sources, fetch_limit);
    upsert_candidates(state, username, candidates, count, existing).await
}

async fn upsert_candidates(
    state: &AppState,
    username: &str,
    candidates: Vec<RecCandidate>,
    count: usize,
    existing: &[search::SongKey],
) -> anyhow::Result<Vec<SongEntry>> {
    let imported = vtrack::imported(&state.db).await.unwrap_or_default();
    let imported_keys: Vec<search::SongKey> = imported
        .iter()
        .map(|t| search::SongKey::new(&t.artist, &t.title, t.duration_ms.map(|ms| ms / 1000)))
        .collect();
    let shown = crate::recs::recently_shown_keys(
        &state.db,
        username,
        state.config.recommendations.shown_cooldown_days,
    )
    .await
    .unwrap_or_default();

    let mut chosen_keys = Vec::new();
    let mut chosen_candidates = Vec::new();
    let mut entries = Vec::new();
    for candidate in candidates {
        let candidate = canonicalize_candidate(state, candidate).await;
        let key = search::SongKey::new(
            &candidate.artist,
            &candidate.title,
            candidate.duration_ms.map(|ms| ms / 1000),
        );
        let duplicate = existing.iter().any(|e| e.matches(&key))
            || imported_keys.iter().any(|e| e.matches(&key))
            || chosen_keys
                .iter()
                .any(|e: &search::SongKey| e.matches(&key));
        if duplicate || shown.contains(&candidate.song_key()) {
            continue;
        }

        let provider = match candidate.provider.as_deref() {
            Some("deezer") => "deezer",
            _ => YTM_PROVIDER,
        };
        let provider_track_id = candidate
            .provider_track_id
            .clone()
            .or_else(|| candidate.video_id.clone())
            .unwrap_or_else(|| format!("rec:{}", candidate.song_key()));
        let track = CatalogTrack {
            provider,
            provider_track_id,
            artist: candidate.artist.clone(),
            title: candidate.title.clone(),
            album: candidate.album.clone(),
            duration_ms: candidate.duration_ms,
            isrc: candidate.isrc.clone(),
            artwork_url: candidate.artwork_url.clone(),
        };
        let id = vtrack::upsert(&state.db, &track).await?;
        if let Some(video_id) = &candidate.video_id {
            let url = watch_url(video_id);
            let _ = vtrack::set_resolution(&state.db, &id, &url, 100, &track.title).await;
        }
        let stored = vtrack::get(&state.db, &id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("virtual track {id} vanished"))?;
        entries.push(SongEntry::from_virtual(&stored, &state.config.streaming));
        chosen_keys.push(key);
        chosen_candidates.push(candidate);
        if entries.len() >= count {
            break;
        }
    }
    let _ = crate::recs::mark_shown(&state.db, username, &chosen_candidates).await;
    Ok(entries)
}

async fn cached_source<F, Fut>(
    state: &AppState,
    source: &str,
    seed_key: &str,
    fetch: F,
) -> anyhow::Result<Vec<RecCandidate>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<RecCandidate>>>,
{
    if let Some(cached) = crate::recs::cache_get(
        &state.db,
        source,
        seed_key,
        state.config.recommendations.cache_ttl_hours,
    )
    .await?
    {
        return Ok(cached);
    }
    let candidates = fetch().await?;
    crate::recs::cache_set(&state.db, source, seed_key, &candidates).await?;
    Ok(candidates)
}

async fn canonicalize_candidate(state: &AppState, candidate: RecCandidate) -> RecCandidate {
    if candidate.provider.as_deref() == Some("deezer")
        || state.config.recommendations.weight_deezer <= 0.0
    {
        return candidate;
    }
    match crate::recs::deezer::canonicalize(
        &state.http,
        &state.config.external_search.api_base_deezer,
        &candidate,
    )
    .await
    {
        Ok(Some(mut canonical)) => {
            canonical.video_id = candidate.video_id;
            canonical
        }
        Ok(None) | Err(_) => candidate,
    }
}

/// Find a YouTube *Music* video id to seed a radio from. A YTM radio is only
/// as good as its seed: a plain-YouTube id (what yt-dlp search yields, and
/// what a non-YTM track's `resolved_url` holds) seeds the YouTube *autoplay*
/// queue — official-video/slowed+reverb/parody uploads — instead of a clean
/// YT Music radio. So ytmusic-origin seeds use their music id directly, and
/// everything else is resolved to one via a YTM song search.
async fn seed_video_id(state: &AppState, seed: &SeedTrack) -> anyhow::Result<String> {
    if seed.provider == YTM_PROVIDER && !seed.provider_track_id.is_empty() {
        return Ok(seed.provider_track_id.clone());
    }
    let query = format!("{} {}", seed.artist, seed.title);
    let hits = crate::recs::ytm::song_search(
        &state.yt_http,
        &state.config.recommendations.ytm_api_base,
        &query,
        1,
    )
    .await?;
    hits.into_iter()
        .find_map(|candidate| candidate.video_id)
        .ok_or_else(|| anyhow::anyhow!("no YTM song match to seed radio for {query}"))
}

async fn fetch_real_seed(state: &AppState, id: &str) -> anyhow::Result<SeedTrack> {
    let encoded_id: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
    let url = format!(
        "{}/rest/getSong?{}&id={encoded_id}&f=json",
        state.config.navidrome.base_url.trim_end_matches('/'),
        auth::admin_auth_query(&state.config.navidrome)
    );
    let value: serde_json::Value = state
        .http
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let song = &value["subsonic-response"]["song"];
    let artist = song["artist"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("getSong response missing artist"))?;
    let title = song["title"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("getSong response missing title"))?;
    Ok(SeedTrack {
        artist: artist.to_string(),
        title: title.to_string(),
        provider: "navidrome".into(),
        provider_track_id: id.to_string(),
    })
}

fn inject_entries(
    status: axum::http::StatusCode,
    mut headers: axum::http::HeaderMap,
    body_text: &str,
    kind: RecKind,
    format: Format,
    entries: &[SongEntry],
) -> Response {
    if entries.is_empty() {
        return search::raw_response(status, headers, body_text.as_bytes().to_vec());
    }
    let new_body = match format {
        Format::Json => search::inject_json(body_text, kind.result_key(), entries),
        Format::Xml => search::inject_xml(body_text, kind.result_key(), entries),
    };
    match new_body {
        Ok(new_body) => {
            headers.remove(CONTENT_LENGTH);
            headers.remove(CONTENT_TYPE);
            let mut response = Response::builder()
                .status(status)
                .header(CONTENT_TYPE, format.content_type())
                .body(Body::from(new_body))
                .unwrap();
            response.headers_mut().extend(headers);
            response
        }
        Err(error) => {
            tracing::warn!(%error, "recommendation injection failed; returning original body");
            search::raw_response(status, headers, body_text.as_bytes().to_vec())
        }
    }
}

fn songs_payload(key: &'static str, entries: Vec<SongEntry>) -> Payload<'static> {
    let json = serde_json::json!({
        "song": entries.iter().map(SongEntry::to_json).collect::<Vec<_>>()
    });
    Payload {
        key,
        json,
        write_xml: Box::new(move |writer| {
            writer
                .write_event(Event::Start(BytesStart::new(key)))
                .unwrap();
            for entry in &entries {
                entry.write_xml(writer, "song");
            }
            writer.write_event(Event::End(BytesEnd::new(key))).unwrap();
        }),
    }
}

fn existing_keys(body: &str, result_key: &str, format: Format) -> Vec<search::SongKey> {
    match format {
        Format::Json => search::existing_songs_json(body, result_key),
        Format::Xml => search::existing_songs_xml(body),
    }
}

fn ok_body_text(status: axum::http::StatusCode, body: &[u8], format: Format) -> Option<String> {
    std::str::from_utf8(body)
        .ok()
        .filter(|text| status.is_success() && search::is_ok_response(text, format))
        .map(str::to_string)
}

fn requested_count(raw: Option<&str>, max_results: u32) -> usize {
    raw.and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(max_results)
        .min(max_results)
        .max(0) as usize
}

fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Streaming;

    fn entry(id: &str, title: &str) -> SongEntry {
        SongEntry::from_virtual(
            &VirtualTrack {
                id: id.into(),
                provider: YTM_PROVIDER.into(),
                provider_track_id: "vid".into(),
                artist: "Artist".into(),
                title: title.into(),
                album: None,
                duration_ms: Some(180_000),
                isrc: None,
                artwork_url: None,
                status: "virtual".into(),
                real_subsonic_id: None,
                resolved_url: None,
                resolved_score: None,
                resolved_title: None,
                resolved_at_epoch: None,
            },
            &Streaming::default(),
        )
    }

    #[test]
    fn payload_renders_expected_container() {
        let envelope = crate::subsonic::Envelope::default();
        let response = envelope.render_ok(
            Format::Json,
            Some(songs_payload("similarSongs2", vec![entry("sgr_x", "A")])),
        );
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
