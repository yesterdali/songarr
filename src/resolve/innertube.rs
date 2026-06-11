//! Native innertube fast path: one POST to `youtubei/v1/player`
//! impersonating the iOS client returns directly fetchable audio URLs —
//! no yt-dlp, no signature solving, ~300ms instead of ~2–3s.
//!
//! This is the deliberately fragile part of the latency story: Google
//! rotates client requirements every few months. Everything here is
//! best-effort — ANY failure makes the caller fall back to the yt-dlp
//! pipe, so breakage degrades speed, never availability. When it breaks,
//! refresh CLIENT_* below from yt-dlp's `_base.py` (ios client) and rebuild.

use serde_json::json;

/// Keep in sync with yt-dlp's ios client definition when this stops working.
const CLIENT_VERSION: &str = "20.20.7";
/// The media GET must reuse this UA — googlevideo 403s on a mismatch
/// between the URL-requesting client and the downloader.
pub const CLIENT_USER_AGENT: &str =
    "com.google.ios.youtube/20.20.7 (iPhone16,2; U; CPU iOS 18_1_0 like Mac OS X;)";

#[derive(Debug, Clone)]
pub struct DirectAudio {
    pub url: String,
    pub mime_type: String,
    pub bitrate: i64,
    /// Total bytes of the stream (format's `contentLength`). Needed to drive
    /// the chunk loop: query-range responses are `200` with no Content-Range,
    /// so there's no other signal for where the stream ends.
    pub content_length: Option<u64>,
}

/// Extract the video id from a watch URL (or accept a bare 11-char id).
pub fn video_id(url: &str) -> Option<String> {
    if let Some(query) = url.split('?').nth(1) {
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if key == "v" && looks_like_id(&value) {
                return Some(value.into_owned());
            }
        }
    }
    if let Some(rest) = url.split("youtu.be/").nth(1) {
        let id = rest.split(['?', '&', '/']).next().unwrap_or("");
        if looks_like_id(id) {
            return Some(id.to_string());
        }
    }
    if looks_like_id(url) {
        return Some(url.to_string());
    }
    None
}

fn looks_like_id(s: &str) -> bool {
    s.len() == 11
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Ask innertube for the best directly-fetchable audio format of a video.
pub async fn direct_audio(
    http: &reqwest::Client,
    api_base: &str,
    watch_url: &str,
) -> anyhow::Result<DirectAudio> {
    let id = video_id(watch_url).ok_or_else(|| anyhow::anyhow!("no video id in {watch_url}"))?;

    let body = json!({
        "context": {
            "client": {
                "clientName": "IOS",
                "clientVersion": CLIENT_VERSION,
                "deviceMake": "Apple",
                "deviceModel": "iPhone16,2",
                "osName": "iPhone",
                "osVersion": "18.1.0.22B83",
                "hl": "en",
                "gl": "US",
                "utcOffsetMinutes": 0
            }
        },
        "videoId": id,
        "contentCheckOk": true,
        "racyCheckOk": true
    });

    let response: serde_json::Value = http
        .post(format!(
            "{}/youtubei/v1/player?prettyPrint=false",
            api_base.trim_end_matches('/')
        ))
        .header("User-Agent", CLIENT_USER_AGENT)
        .header("X-Youtube-Client-Name", "5")
        .header("X-Youtube-Client-Version", CLIENT_VERSION)
        .json(&body)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let status = response["playabilityStatus"]["status"]
        .as_str()
        .unwrap_or("MISSING");
    anyhow::ensure!(
        status == "OK",
        "playability {status}: {}",
        response["playabilityStatus"]["reason"]
            .as_str()
            .unwrap_or("")
    );

    let formats = response["streamingData"]["adaptiveFormats"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    pick_best_audio(&formats)
        .ok_or_else(|| anyhow::anyhow!("no directly fetchable audio format (sig-protected?)"))
}

/// Highest-bitrate audio format that carries a plain `url` (formats with
/// only `signatureCipher` need the JS challenge dance — yt-dlp territory).
fn pick_best_audio(formats: &[serde_json::Value]) -> Option<DirectAudio> {
    formats
        .iter()
        .filter_map(|f| {
            let mime = f["mimeType"].as_str()?;
            if !mime.starts_with("audio/") {
                return None;
            }
            let url = f["url"].as_str()?;
            // contentLength is a stringified integer in the player response.
            let content_length = f["contentLength"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| f["contentLength"].as_u64());
            Some(DirectAudio {
                url: url.to_string(),
                mime_type: mime.to_string(),
                bitrate: f["bitrate"].as_i64().or(f["averageBitrate"].as_i64())?,
                content_length,
            })
        })
        .max_by_key(|f| f.bitrate)
}

/// googlevideo serves these iOS-client URLs only when the range is given as
/// a `&range=start-end` QUERY PARAMETER — an HTTP `Range:` header 403s — and
/// it caps each response at ~1 MiB: a span above that 403s too. So we fetch
/// in 1 MiB query-range chunks. Responses are `200` (no Content-Range), so
/// the loop is driven by the format's contentLength.
const CHUNK_BYTES: u64 = 1024 * 1024;

/// Bytes we must successfully fetch inline before committing to this source.
/// Without a PO token googlevideo serves only the first ~1 MiB of a stream
/// and 403s everything past it; reading two chunks clears that window, so a
/// token wall surfaces HERE (→ caller falls back to yt-dlp for the whole
/// song) instead of mid-stream as a truncated, unrecoverable response.
const VALIDATE_BYTES: u64 = 2 * CHUNK_BYTES;

/// Open the media URL as a sequential chunked reader. Chunks up to
/// `VALIDATE_BYTES` are fetched inline so a token wall (or any early failure)
/// surfaces to the caller, which falls back to yt-dlp; the remainder streams
/// in the background.
///
/// `total` is the format's contentLength. When known it bounds the loop
/// exactly; when absent we stop on the first short chunk (body smaller than
/// the requested span), which marks EOF.
pub async fn open_media_stream(
    http: &reqwest::Client,
    media_url: &str,
    total: Option<u64>,
) -> anyhow::Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
    // Inline validation window: fetch whole chunks until we've cleared the
    // tokenless free window (or reached EOF). Any 403/short read here is a
    // hard error → the caller degrades to yt-dlp, never a truncated stream.
    let mut prefix: Vec<axum::body::Bytes> = Vec::new();
    let mut offset: u64 = 0;
    let mut finished = false;
    loop {
        let resp = range_request(http, media_url, offset, total).await?;
        let status = resp.status();
        anyhow::ensure!(status.is_success(), "media fetch returned {status}");
        let bytes = resp.bytes().await?;
        let n = bytes.len() as u64;
        offset += n;
        if n > 0 {
            prefix.push(bytes);
        }
        // EOF: hit the known total, or (total unknown) a short/empty chunk.
        if total.map_or(n < CHUNK_BYTES, |t| offset >= t) {
            finished = true;
            break;
        }
        if offset >= VALIDATE_BYTES {
            break; // cleared the free window; stream the rest in background
        }
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(8);
    if !finished {
        let client = http.clone();
        let url = media_url.to_string();
        tokio::spawn(async move {
            let mut offset = offset;
            loop {
                match range_request(&client, &url, offset, total).await {
                    Ok(r) if r.status().is_success() => {
                        let mut chunk_bytes: u64 = 0;
                        let mut stream = r.bytes_stream();
                        while let Some(item) = tokio_stream::StreamExt::next(&mut stream).await {
                            match item {
                                Ok(bytes) => {
                                    offset += bytes.len() as u64;
                                    chunk_bytes += bytes.len() as u64;
                                    if tx.send(Ok(bytes)).await.is_err() {
                                        return; // reader dropped (client gone)
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(std::io::Error::other(e))).await;
                                    return;
                                }
                            }
                        }
                        let done = total.map_or(chunk_bytes < CHUNK_BYTES, |t| offset >= t);
                        if done {
                            return; // channel closes → reader sees EOF
                        }
                    }
                    Ok(r) => {
                        let _ = tx
                            .send(Err(std::io::Error::other(format!(
                                "chunk at {offset} returned {}",
                                r.status()
                            ))))
                            .await;
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(std::io::Error::other(e))).await;
                        return;
                    }
                }
            }
        });
    }

    use tokio_stream::StreamExt;
    let body = tokio_stream::iter(prefix.into_iter().map(Ok))
        .chain(tokio_stream::wrappers::ReceiverStream::new(rx));
    Ok(Box::new(tokio_util::io::StreamReader::new(body)))
}

/// Fetch one chunk starting at `offset`, asking for a ≤1 MiB span via the
/// `&range=` query param. The end is clamped to the last byte when the total
/// is known, so the final request never overshoots into a 403.
async fn range_request(
    http: &reqwest::Client,
    url: &str,
    offset: u64,
    total: Option<u64>,
) -> reqwest::Result<reqwest::Response> {
    let mut end = offset + CHUNK_BYTES - 1;
    if let Some(t) = total {
        end = end.min(t.saturating_sub(1));
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    http.get(format!("{url}{sep}range={offset}-{end}"))
        .header(reqwest::header::USER_AGENT, CLIENT_USER_AGENT)
        .send()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_video_ids() {
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            video_id("https://www.youtube.com/watch?app=m&v=dQw4w9WgXcQ&t=4s").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            video_id("https://youtu.be/dQw4w9WgXcQ?si=xyz").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(video_id("dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://example.com/nope"), None);
    }

    #[test]
    fn picks_highest_bitrate_plain_url_audio() {
        let formats = vec![
            json!({"mimeType": "video/mp4; codecs=\"avc1\"", "url": "http://x/video", "bitrate": 900_000}),
            json!({"mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "url": "http://x/aac", "bitrate": 128_000}),
            json!({"mimeType": "audio/webm; codecs=\"opus\"", "url": "http://x/opus", "bitrate": 160_000, "contentLength": "2768759"}),
            // sig-protected: no plain url → must be skipped even at 999k
            json!({"mimeType": "audio/webm; codecs=\"opus\"", "signatureCipher": "s=..", "bitrate": 999_000}),
        ];
        let best = pick_best_audio(&formats).unwrap();
        assert_eq!(best.url, "http://x/opus");
        assert_eq!(best.bitrate, 160_000);
        // contentLength is a stringified int in the player response.
        assert_eq!(best.content_length, Some(2_768_759));
        assert!(pick_best_audio(&[]).is_none());
    }
}
