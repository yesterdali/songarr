//! YouTube Music song radio via innertube `next` — the queue YTM builds when
//! you tap "start radio" on a track. No auth, no key; candidates arrive with
//! their videoId, i.e. pre-resolved.
//!
//! Deliberately fragile, like `resolve::innertube`: Google rotates response
//! shapes. The parser therefore searches the JSON tree for
//! `playlistPanelVideoRenderer` objects instead of hardcoding a path, and ANY
//! failure returns an error the caller treats as "this voter abstains".

use serde_json::{json, Value};

use super::RecCandidate;

/// WEB_REMIX is YTM's web client; keep version loosely current when this
/// stops working (yt-dlp's `_base.py` has the canonical value).
const CLIENT_VERSION: &str = "1.20250101.01.00";

/// Fetch the radio queue seeded by a video. Returns candidates in queue
/// order, seed itself excluded.
pub async fn radio(
    http: &reqwest::Client,
    api_base: &str,
    seed_video_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let body = json!({
        "context": {
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": CLIENT_VERSION,
                "hl": "en",
                "gl": "US"
            }
        },
        "videoId": seed_video_id,
        "playlistId": format!("RDAMVM{seed_video_id}"),
        "isAudioOnly": true,
        "tunerSettingValue": "AUTOMIX_SETTING_NORMAL"
    });

    let response: Value = http
        .post(format!(
            "{}/youtubei/v1/next?prettyPrint=false",
            api_base.trim_end_matches('/')
        ))
        .header("Origin", "https://music.youtube.com")
        .header("Referer", "https://music.youtube.com/")
        .header("X-Youtube-Client-Name", "67")
        .header("X-Youtube-Client-Version", CLIENT_VERSION)
        .json(&body)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let candidates = parse_queue(&response, seed_video_id, limit);
    anyhow::ensure!(
        !candidates.is_empty(),
        "radio queue empty (schema rotated?)"
    );
    Ok(candidates)
}

/// Fetch top-ish songs for an artist through YouTube Music search. YTM's
/// exact ranking is opaque; for R1 this gives us a no-auth source that
/// clients can browse through `getTopSongs`.
pub async fn top_songs(
    http: &reqwest::Client,
    api_base: &str,
    artist: &str,
    limit: usize,
) -> anyhow::Result<Vec<RecCandidate>> {
    let body = json!({
        "context": {
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": CLIENT_VERSION,
                "hl": "en",
                "gl": "US"
            }
        },
        "query": artist,
        // Songs filter. Kept best-effort: if YTM rotates this, parse failure
        // only removes this voter for the request.
        "params": "EgWKAQIIAWoKEAMQBBAJEAoQBQ%3D%3D"
    });

    let response: Value = http
        .post(format!(
            "{}/youtubei/v1/search?prettyPrint=false",
            api_base.trim_end_matches('/')
        ))
        .header("Origin", "https://music.youtube.com")
        .header("Referer", "https://music.youtube.com/")
        .header("X-Youtube-Client-Name", "67")
        .header("X-Youtube-Client-Version", CLIENT_VERSION)
        .json(&body)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let candidates = parse_search_results(&response, limit);
    anyhow::ensure!(
        !candidates.is_empty(),
        "top songs search empty (schema rotated?)"
    );
    Ok(candidates)
}

/// Collect every `playlistPanelVideoRenderer` anywhere in the response.
fn parse_queue(response: &Value, seed_video_id: &str, limit: usize) -> Vec<RecCandidate> {
    let mut renderers = Vec::new();
    collect_renderers(response, &mut renderers);
    renderers
        .into_iter()
        .filter_map(candidate_from_renderer)
        .filter(|c| c.video_id.as_deref() != Some(seed_video_id))
        .take(limit)
        .collect()
}

fn parse_search_results(response: &Value, limit: usize) -> Vec<RecCandidate> {
    let mut out = Vec::new();
    collect_candidates(response, &mut out);
    out.into_iter().take(limit).collect()
}

fn collect_renderers<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "playlistPanelVideoRenderer" {
                    out.push(child);
                } else {
                    collect_renderers(child, out);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_renderers(child, out);
            }
        }
        _ => {}
    }
}

fn collect_candidates(value: &Value, out: &mut Vec<RecCandidate>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "playlistPanelVideoRenderer" {
                    if let Some(candidate) = candidate_from_renderer(child) {
                        out.push(candidate);
                    }
                } else if key == "musicResponsiveListItemRenderer" {
                    if let Some(candidate) = candidate_from_music_item(child) {
                        out.push(candidate);
                    }
                } else {
                    collect_candidates(child, out);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_candidates(child, out);
            }
        }
        _ => {}
    }
}

fn candidate_from_renderer(renderer: &Value) -> Option<RecCandidate> {
    let video_id = renderer["videoId"].as_str()?.to_string();
    let title = runs_text(&renderer["title"]);
    // longBylineText runs: artist(s), then " • "-separated album/views/year.
    // Everything before the first separator is the artist.
    let artist = byline_artist(&renderer["longBylineText"]);
    if title.is_empty() || artist.is_empty() {
        return None;
    }
    let duration_ms = renderer["lengthText"]["runs"][0]["text"]
        .as_str()
        .or_else(|| renderer["lengthText"]["simpleText"].as_str())
        .and_then(parse_clock);
    Some(RecCandidate {
        artist,
        title,
        album: None,
        duration_ms,
        isrc: None,
        artwork_url: None,
        provider: Some("ytmusic".into()),
        provider_track_id: Some(video_id.clone()),
        video_id: Some(video_id),
    })
}

fn candidate_from_music_item(renderer: &Value) -> Option<RecCandidate> {
    let video_id = find_video_id(renderer)?;
    let flex = renderer["flexColumns"].as_array()?;
    let title = flex_column_text(flex.first()?);
    let mut artist = flex
        .get(1)
        .map(flex_column_text)
        .map(|text| text.split('•').next().unwrap_or("").trim().to_string())
        .unwrap_or_default();
    if artist.is_empty() {
        artist = byline_artist(&renderer["longBylineText"]);
    }
    if title.is_empty() || artist.is_empty() {
        return None;
    }
    let duration_ms = renderer["fixedColumns"]
        .as_array()
        .and_then(|cols| cols.first())
        .map(fixed_column_text)
        .filter(|text| !text.is_empty())
        .as_deref()
        .and_then(parse_clock);
    Some(RecCandidate {
        artist,
        title,
        album: None,
        duration_ms,
        isrc: None,
        artwork_url: None,
        provider: Some("ytmusic".into()),
        provider_track_id: Some(video_id.clone()),
        video_id: Some(video_id),
    })
}

fn find_video_id(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(id) = map
                .get("videoId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                return Some(id.to_string());
            }
            for child in map.values() {
                if let Some(id) = find_video_id(child) {
                    return Some(id);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(find_video_id),
        _ => None,
    }
}

fn flex_column_text(value: &Value) -> String {
    runs_text(&value["musicResponsiveListItemFlexColumnRenderer"]["text"])
}

fn fixed_column_text(value: &Value) -> String {
    runs_text(&value["musicResponsiveListItemFixedColumnRenderer"]["text"])
}

fn runs_text(value: &Value) -> String {
    value["runs"]
        .as_array()
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r["text"].as_str())
                .collect::<String>()
        })
        .or_else(|| value["simpleText"].as_str().map(str::to_string))
        .unwrap_or_default()
}

fn byline_artist(value: &Value) -> String {
    let Some(runs) = value["runs"].as_array() else {
        return runs_text(value);
    };
    let mut artist = String::new();
    for run in runs {
        let text = run["text"].as_str().unwrap_or("");
        if text.trim() == "•" {
            break;
        }
        artist.push_str(text);
    }
    artist.trim().to_string()
}

/// "3:45" / "1:02:03" → milliseconds.
fn parse_clock(text: &str) -> Option<i64> {
    let mut secs: i64 = 0;
    for part in text.split(':') {
        secs = secs.checked_mul(60)? + part.trim().parse::<i64>().ok()?;
    }
    Some(secs * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renderer(video_id: &str, title: &str, artist: &str, length: &str) -> Value {
        json!({
            "playlistPanelVideoRenderer": {
                "videoId": video_id,
                "title": {"runs": [{"text": title}]},
                "longBylineText": {"runs": [
                    {"text": artist},
                    {"text": " • "},
                    {"text": "Some Album"},
                    {"text": " • "},
                    {"text": "2026"}
                ]},
                "lengthText": {"runs": [{"text": length}]}
            }
        })
    }

    #[test]
    fn parses_queue_excluding_seed_anywhere_in_tree() {
        // Renderers nested at uneven depths — the walker must find them all.
        let response = json!({
            "contents": {"deeply": {"nested": [
                renderer("seed0000000", "Seed Song", "Seed Artist", "3:00"),
                {"wrapped": renderer("cand0000001", "Notion", "The Rare Occasions", "3:15")},
            ]}},
            "elsewhere": [renderer("cand0000002", "Где нас нет", "Oxxxymiron", "4:05")]
        });
        let queue = parse_queue(&response, "seed0000000", 10);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].title, "Notion");
        assert_eq!(queue[0].artist, "The Rare Occasions");
        assert_eq!(queue[0].video_id.as_deref(), Some("cand0000001"));
        assert_eq!(queue[0].duration_ms, Some(195_000));
        assert_eq!(queue[1].artist, "Oxxxymiron");
    }

    #[test]
    fn respects_limit_and_skips_malformed_entries() {
        let response = json!([
            {"playlistPanelVideoRenderer": {"videoId": "novideotitle"}}, // no title
            renderer("cand0000001", "A", "X", "3:00"),
            renderer("cand0000002", "B", "Y", "3:00"),
            renderer("cand0000003", "C", "Z", "3:00"),
        ]);
        let queue = parse_queue(&response, "unrelated000", 2);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].title, "A");
    }

    #[test]
    fn multi_artist_byline_and_long_durations() {
        let r = json!({
            "videoId": "cand0000009",
            "title": {"runs": [{"text": "Collab"}]},
            "longBylineText": {"runs": [
                {"text": "Artist One"}, {"text": " & "}, {"text": "Artist Two"},
                {"text": " • "}, {"text": "Album"}
            ]},
            "lengthText": {"simpleText": "1:02:03"}
        });
        let c = candidate_from_renderer(&r).unwrap();
        assert_eq!(c.artist, "Artist One & Artist Two");
        assert_eq!(c.duration_ms, Some(3_723_000));
    }

    #[test]
    fn parses_music_search_items() {
        let response = json!({
            "contents": [{
                "musicResponsiveListItemRenderer": {
                    "flexColumns": [
                        {"musicResponsiveListItemFlexColumnRenderer": {
                            "text": {"runs": [{"text": "Mock Hit"}]}
                        }},
                        {"musicResponsiveListItemFlexColumnRenderer": {
                            "text": {"runs": [
                                {"text": "Mock Artist"},
                                {"text": " • "},
                                {"text": "Song"}
                            ]}
                        }}
                    ],
                    "fixedColumns": [{
                        "musicResponsiveListItemFixedColumnRenderer": {
                            "text": {"runs": [{"text": "3:04"}]}
                        }
                    }],
                    "playlistItemData": {"videoId": "top00000001"}
                }
            }]
        });
        let candidates = parse_search_results(&response, 10);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].title, "Mock Hit");
        assert_eq!(candidates[0].artist, "Mock Artist");
        assert_eq!(candidates[0].duration_ms, Some(184_000));
        assert_eq!(candidates[0].video_id.as_deref(), Some("top00000001"));
    }
}
