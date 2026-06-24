//! Turn a user-pasted music link (YouTube / Yandex Music / VK) into a songarr
//! virtual track, so the existing stream → stage → import → scan pipeline can
//! play and ingest it. No new download path: we only construct a properly
//! populated [`VirtualTrack`] and let streaming its `sgr_` id do the rest.
//!
//! - YouTube / VK: pin the exact URL via `set_resolution` (score 100) so
//!   `resolve_cached` skips the `ytsearch` and yt-dlp fetches that link.
//! - Yandex: set `provider = "yandex"` + the parsed track id, so the existing
//!   Yandex branch in `stream_source` downloads it by id (correct for geo —
//!   resolution runs on this server, which holds the token).

use anyhow::{anyhow, bail, ensure};

use crate::vtrack::{self, CatalogTrack};
use crate::AppState;

/// Result of ingesting a link: a streamable virtual track id + display fields.
pub struct Ingested {
    pub id: String,
    pub artist: String,
    pub title: String,
    pub provider: &'static str,
}

#[derive(Debug)]
enum Parsed {
    /// Pin this canonical watch URL; yt-dlp fetches it directly.
    YouTube { watch_url: String },
    /// VK audio id `<owner>_<id>`; resolved via the VK helper (yt-dlp has no
    /// VK audio extractor).
    Vk { track_id: String },
    /// Resolve by Yandex track id via the helper (token + Russian egress).
    Yandex { track_id: String },
}

fn yt_watch(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// Detect provider + extract the canonical id/URL. Errors prefixed with
/// "unsupported link" map to a 400 at the HTTP layer.
fn parse(raw: &str) -> anyhow::Result<Parsed> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|_| anyhow!("unsupported link: not a valid URL"))?;
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_lowercase();
    let segments: Vec<String> = url
        .path_segments()
        .map(|segs| segs.map(|s| s.to_string()).collect())
        .unwrap_or_default();

    // --- YouTube ---
    if host == "youtu.be" {
        let id = segments.first().cloned().unwrap_or_default();
        ensure!(!id.is_empty(), "unsupported link: missing YouTube id");
        return Ok(Parsed::YouTube {
            watch_url: yt_watch(&id),
        });
    }
    if host.ends_with("youtube.com") {
        if let Some((_, v)) = url.query_pairs().find(|(k, _)| k == "v") {
            ensure!(!v.is_empty(), "unsupported link: empty YouTube id");
            return Ok(Parsed::YouTube {
                watch_url: yt_watch(&v),
            });
        }
        // /shorts/<id>, /embed/<id>, /live/<id>
        if let [kind, id, ..] = segments.as_slice() {
            if matches!(kind.as_str(), "shorts" | "embed" | "live") && !id.is_empty() {
                return Ok(Parsed::YouTube {
                    watch_url: yt_watch(id),
                });
            }
        }
        bail!("unsupported link: could not find a YouTube video id");
    }

    // --- Yandex Music: /album/<a>/track/<t> or /track/<t> ---
    if host.starts_with("music.yandex.") {
        if let Some(pos) = segments.iter().position(|s| s == "track") {
            if let Some(id) = segments.get(pos + 1) {
                ensure!(!id.is_empty(), "unsupported link: missing Yandex track id");
                return Ok(Parsed::Yandex {
                    track_id: id.clone(),
                });
            }
        }
        bail!("unsupported link: Yandex URL must point to a track");
    }

    // --- VK Music: vk.com/audio<owner>_<id> (also ?z=audio…) ---
    if host == "vk.com" || host == "vk.ru" || host == "m.vk.com" || host.ends_with(".vk.com") {
        if let Some(track_id) = parse_vk_audio_id(&url) {
            return Ok(Parsed::Vk { track_id });
        }
        bail!("unsupported link: VK URL must point to an audio track (vk.com/audio…)");
    }

    bail!("unsupported link: {host} is not YouTube, Yandex Music, or VK")
}

/// Extract a VK audio id (`<owner>_<id>` with optional `_<hash>`) from the path
/// or `z=` query of a VK link, e.g. `vk.com/audio-2001262717_136262717`.
fn parse_vk_audio_id(url: &reqwest::Url) -> Option<String> {
    let haystacks = std::iter::once(url.path().to_string())
        .chain(url.query_pairs().map(|(_, v)| v.to_string()));
    for hay in haystacks {
        if let Some(idx) = hay.find("audio") {
            let rest = &hay[idx + "audio".len()..];
            let id: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if is_vk_audio_id(&id) {
                return Some(id);
            }
        }
    }
    None
}

/// `<owner>_<id>`: owner is an optionally-negative integer, id an integer.
fn is_vk_audio_id(s: &str) -> bool {
    let mut parts = s.splitn(3, '_');
    let owner = parts.next().unwrap_or("");
    let aid = parts.next().unwrap_or("");
    let owner_digits = owner.strip_prefix('-').unwrap_or(owner);
    !owner_digits.is_empty()
        && owner_digits.bytes().all(|b| b.is_ascii_digit())
        && !aid.is_empty()
        && aid.bytes().all(|b| b.is_ascii_digit())
}

/// Public entry point used by the HTTP handler.
pub async fn build_from_url(state: &AppState, raw_url: &str) -> anyhow::Result<Ingested> {
    match parse(raw_url)? {
        Parsed::YouTube { watch_url } => pinned_ytdlp(state, "youtube", &watch_url).await,
        Parsed::Vk { track_id } => vk_virtual(state, &track_id).await,
        Parsed::Yandex { track_id } => yandex_virtual(state, &track_id).await,
    }
}

/// YouTube/VK: read metadata once, create the virtual track, and pin the URL so
/// streaming fetches exactly this link (and the score-100 auto-imports it).
async fn pinned_ytdlp(
    state: &AppState,
    provider: &'static str,
    resolved_url: &str,
) -> anyhow::Result<Ingested> {
    let meta = crate::resolve::ytdlp_metadata(&state.config.streaming, resolved_url).await?;
    let catalog = CatalogTrack {
        provider,
        provider_track_id: meta.id.clone(),
        artist: meta.artist.clone(),
        title: meta.title.clone(),
        album: None,
        duration_ms: meta.duration_ms,
        isrc: None,
        artwork_url: meta.artwork_url.clone(),
    };
    let id = vtrack::upsert(&state.db, &catalog).await?;
    vtrack::set_resolution(&state.db, &id, resolved_url, 100, &meta.title).await?;
    Ok(Ingested {
        id,
        artist: meta.artist,
        title: meta.title,
        provider,
    })
}

/// Yandex: create the virtual track keyed by track id; the existing yandex
/// branch in `stream_source` downloads it by id on play.
async fn yandex_virtual(state: &AppState, track_id: &str) -> anyhow::Result<Ingested> {
    ensure!(
        crate::yandex::available(&state.config.yandex),
        "Yandex Music is not enabled on this server"
    );
    // Best-effort metadata; if the helper can't fetch it, fall back to a
    // placeholder and let lofty re-tag from the downloaded file on import.
    let (artist, title, album, duration_ms, isrc, artwork_url) =
        match crate::yandex::track_meta(&state.config.yandex, track_id).await {
            Ok(t) => (
                t.artist,
                t.title,
                t.album,
                t.duration_ms,
                t.isrc,
                t.artwork_url,
            ),
            Err(error) => {
                tracing::warn!(%error, track_id, "Yandex metadata lookup failed; using placeholder");
                (
                    "Unknown artist".to_string(),
                    format!("Yandex track {track_id}"),
                    None,
                    None,
                    None,
                    None,
                )
            }
        };
    let catalog = CatalogTrack {
        provider: crate::yandex::PROVIDER,
        provider_track_id: track_id.to_string(),
        artist: artist.clone(),
        title: title.clone(),
        album,
        duration_ms,
        isrc,
        artwork_url,
    };
    let id = vtrack::upsert(&state.db, &catalog).await?;
    Ok(Ingested {
        id,
        artist,
        title,
        provider: crate::yandex::PROVIDER,
    })
}

/// VK: create the virtual track keyed by audio id; `stream_source`'s vk branch
/// resolves the (HLS) media URL on play.
async fn vk_virtual(state: &AppState, track_id: &str) -> anyhow::Result<Ingested> {
    ensure!(
        crate::vk::available(&state.config.vk),
        "VK Music is not enabled on this server"
    );
    // Fail loudly rather than create a placeholder track: a placeholder would
    // later fall back to a YouTube search on a garbage query ("VK track <id>")
    // and play a random song. A clear error is far better UX.
    let meta = crate::vk::track_meta(&state.config.vk, track_id)
        .await
        .map_err(|error| anyhow!("couldn't read that VK track: {error}"))?;
    let (artist, title, album, duration_ms, artwork_url) =
        (meta.artist, meta.title, meta.album, meta.duration_ms, meta.artwork_url);
    let catalog = CatalogTrack {
        provider: crate::vk::PROVIDER,
        provider_track_id: track_id.to_string(),
        artist: artist.clone(),
        title: title.clone(),
        album,
        duration_ms,
        isrc: None,
        artwork_url,
    };
    let id = vtrack::upsert(&state.db, &catalog).await?;
    Ok(Ingested {
        id,
        artist,
        title,
        provider: crate::vk::PROVIDER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yt(raw: &str) -> String {
        match parse(raw).unwrap() {
            Parsed::YouTube { watch_url } => watch_url,
            other => panic!("expected YouTube, got {}", label(&other)),
        }
    }
    fn label(p: &Parsed) -> &'static str {
        match p {
            Parsed::YouTube { .. } => "youtube",
            Parsed::Vk { .. } => "vk",
            Parsed::Yandex { .. } => "yandex",
        }
    }

    #[test]
    fn youtube_watch_url() {
        assert_eq!(
            yt("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        // extra query params and no www still work
        assert_eq!(
            yt("https://youtube.com/watch?v=abc123&list=PL&t=5"),
            "https://www.youtube.com/watch?v=abc123"
        );
    }

    #[test]
    fn youtube_short_and_shorts() {
        assert_eq!(
            yt("https://youtu.be/dQw4w9WgXcQ"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            yt("https://www.youtube.com/shorts/XyZ12345"),
            "https://www.youtube.com/watch?v=XyZ12345"
        );
    }

    #[test]
    fn yandex_track_urls() {
        match parse("https://music.yandex.ru/album/1234/track/5678").unwrap() {
            Parsed::Yandex { track_id } => assert_eq!(track_id, "5678"),
            other => panic!("expected Yandex, got {}", label(&other)),
        }
        match parse("https://music.yandex.com/track/999").unwrap() {
            Parsed::Yandex { track_id } => assert_eq!(track_id, "999"),
            other => panic!("expected Yandex, got {}", label(&other)),
        }
    }

    fn vk(raw: &str) -> String {
        match parse(raw).unwrap() {
            Parsed::Vk { track_id } => track_id,
            other => panic!("expected VK, got {}", label(&other)),
        }
    }

    #[test]
    fn vk_audio_urls() {
        assert_eq!(
            vk("https://vk.com/audio-2001262717_136262717"),
            "-2001262717_136262717"
        );
        assert_eq!(vk("https://m.vk.com/audio123_456"), "123_456");
        // the `?z=audio…` share form
        assert_eq!(vk("https://vk.com/feed?z=audio-1_2"), "-1_2");
        // VK *video* is not audio → rejected
        assert!(parse("https://vk.com/video-1_2").is_err());
        assert!(parse("https://vk.com/im").is_err());
    }

    #[test]
    fn unsupported_hosts_and_garbage() {
        assert!(parse("https://soundcloud.com/x/y").is_err());
        assert!(parse("not a url").is_err());
        assert!(parse("https://music.yandex.ru/artist/42").is_err());
        // errors are user-facing and prefixed for the 400 mapping
        assert!(parse("https://soundcloud.com/x")
            .unwrap_err()
            .to_string()
            .starts_with("unsupported link"));
    }
}
