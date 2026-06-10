# Songarr Proxy — Implementation Plan (v1)

A self-contained implementation plan. Hand this entire file to Claude Code as the project brief.

## 1. What we are building

**Songarr Proxy** is a self-hosted **Subsonic/OpenSubsonic reverse proxy** that sits between
Subsonic clients (Amperfy, Arpeggi, Symfonium, Navidrome web UI, etc.) and a **Navidrome**
server. It adds one superpower to an otherwise unmodified Navidrome setup:

> **Search any song in the world from inside your normal music app. Press play. It starts
> playing within ~2 seconds, and the file is silently saved into the local library so it is
> a normal local track forever after.**

Everything else — library management, Soulseek downloads, quality upgrades, weekly
recommendation playlists — is handled by **external tools that already exist** (Navidrome,
SoulSync, slskd). This proxy deliberately does NOT reimplement them.

```text
iPhone (Amperfy) / Android (Symfonium) / Web
        │  Subsonic API (HTTP)
        ▼
┌──────────────────────────────────────────────┐
│  Songarr Proxy (this project)                │
│                                              │
│  • transparent passthrough → Navidrome       │
│  • search3: inject external catalog results  │
│    as "virtual tracks" (id prefix sgr_)      │
│  • stream?id=sgr_…: yt-dlp ──► ffmpeg ──►    │
│    client (live pipe) + tee ──► staging      │
│  • after stream completes: tag → move into   │
│    /music → trigger Navidrome scan →         │
│    remap virtual id → real id                │
│  • enqueue FLAC upgrade via SoulSync         │
└──────────────────────────────────────────────┘
        │ Subsonic API           │ binaries
        ▼                        ▼
   Navidrome ── /music      yt-dlp, ffmpeg/ffprobe
   (unmodified)             (yt-dlp egress goes via gluetun/VPN)
```

### Explicit non-goals for v1
- No recommendation engine (SoulSync + ListenBrainz handle that).
- No Soulseek client implementation (slskd/SoulSync handle that; we only file upgrade requests).
- No Lidarr integration.
- No custom mobile client; existing Subsonic clients must work **unmodified**.
- No transcoding profiles beyond a single sane default for virtual streams.
- No playlist support for virtual tracks (a virtual track must be played/imported before it
  can live in a playlist — Navidrome owns playlists).

## 2. Stack and conventions

- **Language:** Rust (owner's choice; the original project doc was Rust-based).
  - `tokio`, `axum 0.8`, `tower`/`tower-http`, `reqwest 0.12` (rustls), `serde`/`serde_json`,
    `quick-xml` (Subsonic XML responses), `sqlx 0.8` + SQLite, `tracing`, `thiserror`,
    `lofty` (tag writing), `strsim` + `deunicode` (matching), `tokio::process` (yt-dlp/ffmpeg).
- **Single binary, single crate** to start (`src/` modules, not a workspace). Split later only if needed.
- **SQLite** at `/config/songarr.db`, migrations via `sqlx migrate`.
- External binaries expected on PATH inside the container: `yt-dlp`, `ffmpeg`, `ffprobe`.
- Container: distroless-ish Debian slim; `docker-compose.example.yml` includes navidrome +
  the proxy + gluetun wiring (yt-dlp traffic must be routable through gluetun — implement via
  `YTDLP_PROXY` env passed to `yt-dlp --proxy`, do NOT assume container-level VPN).
- All times UTC ISO-8601. UUIDv4 ids internally.

### Suggested file layout
```text
songarr-proxy/
├── Cargo.toml
├── migrations/
├── docker/
│   ├── Dockerfile
│   └── docker-compose.example.yml
├── config.example.toml
└── src/
    ├── main.rs
    ├── config.rs
    ├── db.rs
    ├── subsonic/          # request parsing, response (de)serialization JSON+XML
    │   ├── mod.rs
    │   ├── types.rs       # subsonic-response envelope, Child/Song, Artist, Album…
    │   └── auth.rs        # u/t/s/p param extraction (passthrough only)
    ├── proxy/             # passthrough + interception routing
    │   ├── mod.rs
    │   ├── passthrough.rs
    │   ├── search.rs      # search3/search2 injection
    │   ├── stream.rs      # virtual stream handler
    │   ├── coverart.rs    # getCoverArt for virtual ids
    │   └── scrobble.rs    # swallow/queue scrobbles for virtual ids
    ├── catalog/           # external metadata search
    │   ├── mod.rs
    │   ├── deezer.rs      # primary: free public API, no key
    │   └── ytmusic.rs     # fallback: yt-dlp ytsearch / ytmusicapi-style
    ├── resolve/           # virtual track -> concrete YouTube URL (scoring)
    ├── ingest/            # tee-to-staging, tag, move to /music, scan trigger, id remap
    ├── upgrade/           # SoulSync wishlist hook (best-effort)
    └── admin/             # tiny status UI + JSON API (jobs, mappings, errors)
```

### config.example.toml
```toml
[server]
bind = "0.0.0.0:4534"          # clients point HERE instead of Navidrome

[navidrome]
base_url = "http://navidrome:4533"
# admin creds used ONLY for triggering scans + post-import verification,
# never for serving user requests (user auth is passed through untouched)
admin_user = "admin"
admin_password = "change-me"

[library]
music_dir = "/music"
ingest_subdir = "_songarr"      # imported singles land in /music/_songarr/Artist/…
staging_dir = "/staging"

[external_search]
enabled = true
provider = "deezer"             # deezer | ytmusic | both
max_results = 8                 # virtual results appended per search
min_query_len = 3

[streaming]
ytdlp_path = "yt-dlp"
ytdlp_proxy = ""                # e.g. "http://gluetun:8888" — REQUIRED in RU deployments
format = "opus"                 # transcode target for live pipe: opus | mp3
bitrate_kbps = 160
max_concurrent = 3
timeout_first_byte_secs = 12    # if yt-dlp produces nothing by then → 503 to client

[ingest]
auto_import = true              # move played tracks into /music
min_score_for_import = 80       # candidate match score gate
write_playlist = "Songarr Played.m3u"  # optional rolling playlist of imports, "" disables

[upgrade]
mode = "none"                   # none | soulsync_wishlist (best-effort, see M6)
soulsync_url = "http://soulsync:8888"

[users]
default_can_trigger_downloads = true
deny = []                       # usernames who get passthrough-only (no virtual results)
```

## 3. Critical protocol knowledge (read before coding)

These are the gotchas that will sink the project if ignored. Treat them as requirements.

1. **Auth is per-request and passed through untouched.** Subsonic clients send
   `u` (user) + either `t`+`s` (md5 token+salt) or `p` (password / enc:hex). The proxy must
   NOT validate these — forward all query params verbatim to Navidrome and let it judge.
   For proxy-originated calls (scan trigger, verification searches) use the configured admin
   account with its own token computation (`t = md5(password + salt)`).
2. **Two response formats.** Clients request `f=json` or default XML. The proxy must parse
   and re-serialize BOTH for intercepted endpoints (search3/search2, getCoverArt, stream,
   scrobble). Everything else is byte-for-byte passthrough — do not re-serialize what you
   don't modify. Preserve the `subsonic-response` envelope fields (`status`, `version`,
   OpenSubsonic extensions like `type`, `serverVersion`, `openSubsonic: true`) exactly as
   Navidrome returned them.
3. **Mind compression.** When intercepting, send `Accept-Encoding: identity` upstream (or
   transparently decompress) before modifying the body, and fix `Content-Length`.
4. **Virtual stream responses cannot honor Range requests.** A live yt-dlp pipe has unknown
   length. Respond `200` with `Transfer-Encoding: chunked`, no `Content-Length`,
   `Accept-Ranges: none`. If a client sends a `Range` header for a virtual id, ignore the
   range and return 200 (most clients cope; document per-client behavior in M7). Once a
   track is imported, all subsequent requests hit the real file via Navidrome and seeking
   works normally.
5. **Virtual id scheme.** `sgr_` + 22-char base62 of a UUID stored in SQLite, mapped to
   provider metadata. Ids must be stable across repeated searches for the same external
   track (key on provider + provider_track_id), otherwise clients' caches break.
6. **Every Subsonic endpoint that can receive an id must tolerate a virtual id** without
   500ing: `getSong` (synthesize), `getCoverArt` (fetch/cache provider artwork),
   `scrobble` (accept, store, replay against real id after import), `star`/`unstar`
   (accept, replay after import), `getLyrics*` (empty success), `download` (treat as stream).
   Anything unhandled: return a well-formed Subsonic error 70 (not found), never a crash.
7. **search3 album/artist injection is OPTIONAL — skip in v1.** Only inject into the `song`
   array. Injecting virtual artists/albums creates a combinatorial id-tolerance problem.
8. **Navidrome scan trigger:** `POST /rest/startScan` (Subsonic endpoint) with admin creds;
   poll `getScanStatus` until `scanning=false`, then resolve the imported file's real id via
   `search3` on the tagged artist+title and update the mapping table.

## 4. Data model (SQLite)

```sql
CREATE TABLE virtual_tracks (
  id TEXT PRIMARY KEY,              -- sgr_…
  provider TEXT NOT NULL,           -- deezer | ytmusic
  provider_track_id TEXT NOT NULL,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  album TEXT,
  duration_ms INTEGER,
  isrc TEXT,
  artwork_url TEXT,
  status TEXT NOT NULL DEFAULT 'virtual',
        -- virtual | streaming | staged | imported | failed
  real_subsonic_id TEXT,            -- filled after import
  fail_reason TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(provider, provider_track_id)
);

CREATE TABLE stream_jobs (
  id TEXT PRIMARY KEY,
  virtual_track_id TEXT NOT NULL REFERENCES virtual_tracks(id),
  requested_by TEXT NOT NULL,       -- subsonic username
  source_url TEXT,                  -- resolved YouTube URL
  match_score INTEGER,
  staging_path TEXT,
  status TEXT NOT NULL,             -- resolving | piping | finalizing | imported
                                    -- | needs_review | failed
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE pending_actions (      -- scrobbles/stars received for virtual ids
  id TEXT PRIMARY KEY,
  virtual_track_id TEXT NOT NULL,
  username TEXT NOT NULL,
  action TEXT NOT NULL,             -- scrobble | star
  payload_json TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE upgrade_requests (
  id TEXT PRIMARY KEY,
  virtual_track_id TEXT NOT NULL,
  status TEXT NOT NULL,             -- pending | sent | done | unsupported
  created_at TEXT NOT NULL
);
```

## 5. Milestones

Work strictly in order. Each milestone ends with its acceptance tests passing and a short
note in `DEVLOG.md` describing decisions/deviations.

### M0 — Skeleton
Axum server, config loading, SQLite + migrations, tracing, Dockerfile, compose example,
`GET /healthz`. **Accept:** container builds; healthz OK; db migrates.

### M1 — Transparent passthrough (the compatibility bedrock)
Reverse-proxy ALL of `/rest/*` (and `/` web UI + `/share/*` if trivial) to Navidrome,
streaming bodies, preserving headers, query params, method, status. No interception yet.
**Accept:**
- Navidrome web UI fully usable through the proxy.
- A real client (at minimum: Symfonium or Amperfy against a test server, plus the
  `subsonic-tests` style curl suite below) browses, plays, seeks, scrobbles through the proxy
  with zero behavioral difference.
- Integration test harness exists: docker-compose with a real Navidrome + ~20 seeded audio
  files (generate sine-wave mp3/flac with ffmpeg in a setup script + tag with lofty or
  ffmpeg metadata) and a Rust integration test suite hitting the proxy with both `f=json`
  and XML for: ping, getLicense, getArtists, getAlbumList2, search3, stream (byte-compare
  against direct Navidrome response), getCoverArt, scrobble.

### M2 — External catalog search + virtual tracks
Intercept `search3` (and `search2`): forward to Navidrome, parse, then if the query looks
like a song search (length ≥ min_query_len) call the catalog provider and **append** up to
`max_results` virtual songs to the `song` list — AFTER deduplicating against (a) Navidrome's
own results and (b) already-imported mappings, using normalized artist+title and duration
±3s. Implement `catalog/deezer.rs` first: `GET https://api.deezer.com/search?q=…` — no API
key, returns artist/title/album/duration/ISRC/artwork. Synthesize Subsonic `song` entries:
`id=sgr_…`, `coverArt=sgr_…`, sane `suffix`/`contentType` (`opus`/`audio/ogg` or per config),
`duration` from provider. Implement `getSong` + `getCoverArt` for virtual ids (cache artwork
on disk). Respect `[users] deny` (passthrough-only users get no injection).
**Accept:** searching an obscure track not in the library shows it in a real client with
artwork and correct duration; searching a track that IS local shows no duplicate; denied
user sees vanilla results; XML and JSON both correct.

### M3 — Stream-on-demand
`stream?id=sgr_…` (and `download`):
1. Resolve: `yt-dlp --default-search ytsearch5 "artist title"` (or direct ytmusic search),
   JSON output; score candidates (title/artist similarity via strsim on deunicoded strings,
   duration delta vs provider duration, uploader contains artist or is "Topic"/"Official",
   penalize live/cover/remix/slowed/nightcore/8d keywords). Pick best; record score+URL.
2. Pipe: `yt-dlp -f bestaudio -o - <url> | ffmpeg -i pipe:0 -map a -c:a libopus -b:a 160k
   -f ogg pipe:1` — spawn via tokio, stream ffmpeg stdout to the HTTP response AND tee the
   *raw yt-dlp output* (pre-transcode, best quality) to `staging_dir/<job>.src`. Yes, this
   runs yt-dlp output through a `tee`: response gets transcoded audio, staging keeps the
   original container for later ffmpeg-remux during ingest.
3. Concurrency gate (`max_concurrent`), first-byte timeout → Subsonic error response,
   kill process group on client disconnect BUT continue the download to staging in the
   background if ≥20% was already transferred (the user pressed play; finish the acquisition).
4. `scrobble`/`star` on virtual ids → store in `pending_actions`, return success.
**Accept:** in a real client, tapping a virtual search result starts audible playback in
< 3s on a decent connection; the staged source file appears and is a valid audio file
(ffprobe); two simultaneous virtual streams work; disconnect mid-play still yields a
complete staged file; `ytdlp_proxy` config is honored (verify with a logging proxy).

### M4 — Ingest: staged file → real library track
Background worker per completed stream_job:
1. ffprobe the staged source; remux/transcode to a final container (keep original codec
   when it's already opus/m4a; never transcode lossy→lossy if avoidable — remux m4a/webm
   audio stream as-is).
2. Tag with lofty: artist, title, album, ISRC if known, plus `COMMENT=songarr:src=<url>`.
3. Move to `/music/<ingest_subdir>/<Artist>/<Artist> - <Title>.<ext>` (sanitize names,
   collision → append short hash).
4. Trigger Navidrome scan (admin creds), poll, then `search3` to find the new real id;
   update `virtual_tracks.real_subsonic_id`, status=imported.
5. Replay `pending_actions` (scrobble/star) against the real id as the original user —
   NOTE: requires that user's token; we don't have their password. Solution: replay using
   the *captured* `u/t/s` params from the original request (tokens are not time-limited in
   Subsonic; store them in pending_actions.payload_json). If replay fails, log and drop.
6. From now on, `stream?id=sgr_…` 302-redirects… no — Subsonic clients don't reliably follow
   redirects: instead the proxy internally rewrites the id and passthrough-streams the real
   file from Navidrome. Same for getSong/getCoverArt. Future `search3` calls already
   dedupe-prefer the real track (M2).
**Accept:** play a virtual track, wait for scan; the track now appears in normal library
browsing; playing it again supports seeking (proves it's served by Navidrome); scrobble
recorded under the requesting user (check Navidrome play count); match score below
`min_score_for_import` → status needs_review, file stays in staging, visible in admin UI.

### M5 — Admin mini-UI + operational safety
Single-page (no framework, or one HTML file + fetch) at `/admin` behind the Navidrome admin
credentials (verify by proxying a `ping` with provided creds and checking the user is admin):
list recent stream_jobs with status/score/source URL, needs_review queue with
approve (run ingest) / reject (delete staging) buttons, error log tail, per-user
download-permission toggles persisted to db (overrides config). Add Prometheus-style
`/metrics` (streams started/completed/failed, first-byte latency histogram).
**Accept:** a needs_review item can be approved end-to-end from the browser; denying a user
takes effect without restart.

### M6 — Quality-upgrade hook (best-effort, time-boxed)
Goal: every imported YouTube-sourced track eventually gets replaced by a proper FLAC via
SoulSync/slskd. SoulSync's integration surface is not guaranteed — investigate in this
order, implement the first that works, and stub the rest:
(a) SoulSync HTTP API endpoint for wishlist-add, if one exists in the deployed version;
(b) writing to a watched file/db that SoulSync's wishlist imports;
(c) fallback: mark `unsupported`, expose the wanted-list as `/admin/wanted.csv` so the
owner can import it into any tool manually.
Also handle the upgrade landing: when a SoulSync-downloaded album version of an ingested
single appears in Navidrome (detect via periodic search3 for imported tracks' artist+title
where the found path is outside ingest_subdir), delete the proxy's single and let the real
album track take over (update mapping). **Accept:** documented behavior for whichever path
worked; duplicate cleanup demonstrably removes the `_songarr` copy when an album lands.

### M7 — Client compatibility matrix + hardening
Test and document (CLIENTS.md) against: Navidrome web UI, Amperfy (iOS), Arpeggi (iOS),
Symfonium (Android), Tempo (Android), Feishin (desktop). For each: virtual search display,
play, seek-attempt behavior, artwork, scrobble, post-import behavior, offline-download of an
imported track. Fix what's fixable (e.g., some clients require `size`/`bitRate` fields on
song entries — synthesize estimates). Add graceful degradation: if yt-dlp starts failing
globally (3 consecutive resolve failures), stop injecting virtual results for 10 minutes
and log loudly, so search stays fast and clean.
**Accept:** CLIENTS.md complete; suite from M1 still green; chaos test (kill ffmpeg
mid-stream, Navidrome down, yt-dlp 403s) never produces a hung client connection > 15s or a
malformed Subsonic response.

## 6. Testing strategy (applies throughout)

- Unit tests for: scoring, id mapping, subsonic (de)serialization round-trips (JSON+XML
  fixtures captured from a real Navidrome), filename sanitization.
- Integration tests run against REAL Navidrome in docker compose (no mocks for Navidrome).
  yt-dlp IS mocked in CI: a fake `yt-dlp` shell script on PATH that emits fixture JSON and
  cats a fixture audio file with throttling, so M3/M4 tests are deterministic and offline.
- One opt-in `--ignored` test suite hits real YouTube for local verification only.

## 7. Definition of done for v1

From a stock iPhone with Amperfy pointed at the proxy URL:
1. Search "<any song not in the library>" → it appears with artwork.
2. Tap play → audio within ~3 seconds.
3. Five minutes later the song exists in normal album/artist browsing, plays with seeking,
   has the play scrobbled, and (if upgrade hook landed) is queued for FLAC replacement.
4. Meanwhile, every other Navidrome feature behaves exactly as without the proxy.
