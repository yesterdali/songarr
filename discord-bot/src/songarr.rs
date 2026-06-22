use anyhow::{anyhow, Context as _};
use rand::Rng;

use crate::store::Link;

#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
}

impl Track {
    fn from_json(value: &serde_json::Value) -> Option<Track> {
        Some(Track {
            id: value.get("id")?.as_str()?.to_string(),
            title: value
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            artist: value
                .get("artist")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown artist")
                .to_string(),
        })
    }

    pub fn label(&self) -> String {
        format!("{} — {}", self.artist, self.title)
    }
}

/// Build a [`Link`] from a fresh login (computes a random salt + token so the
/// raw password is never stored).
pub fn make_link(server_url: &str, username: &str, password: &str) -> Link {
    let salt: String = {
        let mut rng = rand::thread_rng();
        (0..16).map(|_| format!("{:x}", rng.gen_range(0..16))).collect()
    };
    let token = format!("{:x}", md5::compute(format!("{password}{salt}")));
    Link {
        server_url: normalize_server(server_url),
        username: username.trim().to_string(),
        salt,
        token,
    }
}

pub fn normalize_server(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

/// Authenticated Subsonic/Songarr client scoped to one user's link.
pub struct SongarrClient<'a> {
    http: &'a reqwest::Client,
    link: &'a Link,
}

impl<'a> SongarrClient<'a> {
    pub fn new(http: &'a reqwest::Client, link: &'a Link) -> Self {
        Self { http, link }
    }

    fn build_url(&self, path: &str, params: &[(&str, &str)]) -> String {
        let mut url = reqwest::Url::parse(&self.link.server_url)
            .unwrap_or_else(|_| reqwest::Url::parse("http://localhost").unwrap());
        url.set_path(&join_path(url.path(), path));
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("u", &self.link.username);
            q.append_pair("t", &self.link.token);
            q.append_pair("s", &self.link.salt);
            q.append_pair("v", "1.16.1");
            q.append_pair("c", "songarr_discord");
            for (k, v) in params {
                q.append_pair(k, v);
            }
        }
        url.to_string()
    }

    async fn get_json(&self, url: &str) -> anyhow::Result<serde_json::Value> {
        let response = self.http.get(url).send().await?.error_for_status()?;
        let body: serde_json::Value = response.json().await?;
        let status = body
            .pointer("/subsonic-response/status")
            .and_then(|v| v.as_str());
        if status != Some("ok") {
            let message = body
                .pointer("/subsonic-response/error/message")
                .and_then(|v| v.as_str())
                .unwrap_or("request failed");
            return Err(anyhow!(message.to_string()));
        }
        Ok(body)
    }

    pub async fn ping(&self) -> anyhow::Result<()> {
        self.get_json(&self.build_url("/rest/ping", &[("f", "json")]))
            .await
            .map(|_| ())
    }

    pub async fn search_song(&self, query: &str) -> anyhow::Result<Option<Track>> {
        let url = self.build_url(
            "/rest/search3",
            &[("f", "json"), ("query", query), ("songCount", "1"), ("albumCount", "0"), ("artistCount", "0")],
        );
        let body = self.get_json(&url).await?;
        let song = body.pointer("/subsonic-response/searchResult3/song");
        let track = match song {
            Some(serde_json::Value::Array(items)) => items.first().and_then(Track::from_json),
            Some(value @ serde_json::Value::Object(_)) => Track::from_json(value),
            _ => None,
        };
        Ok(track)
    }

    /// Endless Wave recommendations for this user (`/wave/api/next`).
    pub async fn wave_next(&self, seed: Option<&str>, count: u32) -> anyhow::Result<Vec<Track>> {
        let count = count.to_string();
        let mut params = vec![("f", "json"), ("count", count.as_str())];
        if let Some(seed) = seed {
            params.push(("seedId", seed));
        }
        let url = self.build_url("/wave/api/next", &params);
        let response = self.http.get(&url).send().await?.error_for_status()?;
        let body: serde_json::Value = response.json().await?;
        let tracks = body.get("tracks").and_then(|v| v.as_array());
        Ok(tracks
            .map(|items| items.iter().filter_map(Track::from_json).collect())
            .unwrap_or_default())
    }

    /// MP3 stream URL — symphonia decodes MP3 reliably, and songbird re-encodes
    /// to Opus for Discord anyway.
    pub fn stream_url(&self, track: &Track) -> String {
        self.build_url(
            "/rest/stream",
            &[("id", &track.id), ("format", "mp3"), ("maxBitRate", "320")],
        )
    }
}

fn join_path(base: &str, path: &str) -> String {
    let prefix = base.trim_end_matches('/');
    let suffix = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{prefix}{suffix}")
}

pub fn require_server<'a>(provided: Option<&'a str>, default: Option<&'a str>) -> anyhow::Result<&'a str> {
    provided
        .or(default)
        .context("no Songarr URL — pass one to /link or set SONGARR_URL on the bot")
}
