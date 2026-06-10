# DEVLOG

## M0 — Skeleton (2026-06-10)

Done: cargo project (`songarr-proxy`, single crate), config loading, SQLite +
embedded migrations (all four tables from the plan), tracing, `GET /healthz`
(checks DB liveness), Dockerfile, `docker-compose.example.yml`.

Verified: `cargo test` green (config defaults/example/unknown-key tests, db
migration test against a real temp SQLite); local binary serves `/healthz` →
200; Docker image builds; containerized `/healthz` → 200; `yt-dlp`, `ffmpeg`,
`ffprobe` present on PATH in the image.

Decisions / deviations from the plan:

- **Project root is the repo root** (`songarr/`), not a `songarr-proxy/`
  subfolder — the directory is dedicated to this project.
- **Added `db_path` to `[server]`** in the config (default
  `/config/songarr.db`). The plan fixed the DB location but had no config key
  for it; needed for local dev outside the container.
- **Config path** comes from `SONGARR_CONFIG` env (default
  `/config/songarr.toml`). Every section/field has a serde default so a
  minimal config works; unknown keys are rejected (`deny_unknown_fields`) to
  catch typos early.
- **Only M0's dependencies are declared** (axum, tokio, sqlx, serde/toml,
  tracing, thiserror/anyhow, uuid). reqwest, quick-xml, lofty, strsim,
  deunicode etc. get added in the milestone that first uses them, to keep
  builds fast and avoid unused-dep rot.
- Migrations are embedded via `sqlx::migrate!` — no sqlx-cli needed at
  runtime. Added a few indexes (job status, virtual_track FKs) beyond the
  plan's schema.
- yt-dlp in the image is the upstream release zipapp (needs `python3`), not
  the stale Debian package; ffmpeg/ffprobe from Debian bookworm.
- Compose example sets Navidrome `ND_SCANSCHEDULE: "0"` since the proxy will
  trigger scans explicitly (M4).

Next: M1 — transparent passthrough of `/rest/*` + integration test harness
with a real Navidrome and seeded audio files.

## M1 — Transparent passthrough (2026-06-10)

Done: streaming reverse proxy as the router fallback — every path not
explicitly ours (`/healthz`) forwards to Navidrome with method, raw
path+query, headers and body preserved; response streams back unmodified
(works for SSE `/api/events`, gzip assets, range requests). Hop-by-hop
headers (RFC 9110 §7.6.1 + `Connection`-named) stripped both directions;
redirects passed through, never followed. Restructured into lib + thin bin so
tests boot the proxy in-process.

Integration harness: `tests/harness/` — compose with real Navidrome
(admin auto-created via `ND_DEVAUTOCREATEADMINPASSWORD`), `seed.sh` generates
20 tagged sine-wave tracks (mp3+flac, 4 artists/albums, folder cover art) via
ffmpeg. `up.sh` / `down.sh` manage it. Suite (`tests/passthrough.rs`,
`--ignored`): ping, getLicense, getArtists, getAlbumList2, search3,
getCoverArt, stream (byte-compare, `format=raw`), scrobble — each in JSON and
XML, asserting direct-vs-proxied responses identical. Plus root-redirect
passthrough, `/app/` shell, web-UI login POST, gzip assets, 502 on upstream
down, `/healthz` not proxied. 5 consecutive green runs.

Gotchas discovered (cost real debugging time):

- **Navidrome serializes artist `roles` in nondeterministic order** (Go map
  iteration) — two identical requests differ byte-wise. Tests canonicalize
  exactly that (sort string-only JSON arrays / consecutive `<roles>` XML
  elements) before comparing; everything else still byte-strict.
- **Navidrome's SQLite dies with intermittent `disk I/O error` on macOS
  Docker bind mounts.** Harness uses a named volume for `/data`. Symptom was
  flaky auth failures (error 40) and empty search results.
- Request bodies with Content-Length are buffered (≤16 MB — Subsonic posts
  are tiny); chunked bodies stream. GET/HEAD send no body so reqwest doesn't
  emit spurious `Transfer-Encoding: chunked` upstream.

Not done here: real-client verification (Amperfy/Symfonium against this
proxy) — needs the owner's device; suite + web UI strongly suggest it'll
pass. M7 owns the formal client matrix.

Next: M2 — Deezer catalog search + virtual track injection into search3.

## M2 — External catalog search + virtual tracks (2026-06-10)

Done: `search3`/`search2` interception (bare + `.view`) — upstream response
fetched with `Accept-Encoding: identity`, parsed, and up to `max_results`
Deezer hits appended to the `song` array in BOTH formats: JSON via
serde_json Value surgery, XML via quick-xml event-stream injection (handles
normal, self-closing, and absent `searchResultN`). Envelope bytes from
Navidrome are preserved untouched. Dedup: normalized (deunicode, lowercase,
alphanumeric-only) artist+title, duration ±3s when both known — against
Navidrome's results, already-imported mappings, and within the injected set.
Virtual ids `sgr_` + base62(UUIDv4), stable per (provider, provider_track_id)
via UPSERT. `getSong` synthesizes full responses (envelope attrs mirrored
from a cached admin `ping`); `getCoverArt` fetches provider artwork once,
caches at `<db dir>/artwork/`, sniffs content-type. `[users] deny` and
`min_query_len` short-circuit to pure passthrough. Unknown virtual ids →
well-formed error 70, never a 500.

Verified: 25 unit tests; 6 new integration tests against harness Navidrome +
a mock Deezer (injection both formats, id stability, local-track dedup,
denied user, short query, getSong synth + error 70, cover art disk-cache hit
counting); M1 passthrough suite still green; live smoke against the real
Deezer API through the compiled binary (8 virtual results for a
not-in-library track).

Decisions / deviations:

- Failure policy: catalog errors/timeouts (5s) degrade to vanilla Navidrome
  results, logged — search must never break (the M7 trip-breaker will build
  on this).
- `f=jsonp` (and unknown formats) skip interception entirely; those clients
  get vanilla passthrough rather than risking a malformed re-serialization.
- Added hidden config `external_search.api_base_deezer` (default
  `https://api.deezer.com`) so tests inject a local mock; not in the example
  config on purpose.
- Deezer's search endpoint doesn't return ISRC (only the track-detail
  endpoint does) — stored as NULL for now; M4 can backfill at tag time.
- ytmusic provider falls back to Deezer until M3 lands yt-dlp plumbing.
- Symfonium-style quoted queries (`"foo"`) are unwrapped before length
  checks/provider calls.
- `proxy::rewrite_id_and_passthrough` (id-param rewrite, auth preserved) is
  in place for post-import serving; `getSong`/`getCoverArt` already use it
  when `real_subsonic_id` is set, so M4 only has to fill that column.

Next: M3 — stream-on-demand (yt-dlp resolve + scored match, live opus pipe
with tee to staging, concurrency gate, pending scrobbles/stars).

## M3 — Stream-on-demand (2026-06-10)

Done: `stream`/`download` for `sgr_` ids. Resolve via
`yt-dlp --flat-playlist --dump-json ytsearch5:"artist title"`, scored 0–100
(jaro-winkler title/artist similarity on deunicoded strings, duration delta
tiers, Topic/official channel bonus, live/cover/remix/slowed/nightcore/8d/…
keyword penalties — skipped when the wanted title itself contains them).
Pipe: yt-dlp bestaudio → tee (staging keeps the original container) → ffmpeg
→ libopus/mp3 → chunked 200 response with `Accept-Ranges: none`, no
Content-Length; Range headers ignored. Semaphore gate (`max_concurrent`),
first-byte timeout → HTTP 503 with a subsonic error body, 30-min pipeline
cap. Client disconnect: ffmpeg dies of EPIPE and the download continues to
staging when ≥256 KiB was already fetched. `scrobble`/`star`/`unstar` for
virtual ids land in `pending_actions` with the full original query (captured
auth for M4 replay); mixed batches forward real ids upstream.
`getLyricsBySongId` returns empty success. Completed jobs sit at
`finalizing` / track `staged` for the M4 worker.

Verified: 30 unit tests (scoring, flat-JSON parsing); 7 integration tests
with a mock yt-dlp (fixture webm streamed throttled): play+stage (ffprobe
validates the staged copy), two simultaneous streams, disconnect mid-play
still completes staging, 503 on first-byte timeout, pending actions stored
with auth, empty lyrics, `--proxy` passed to both yt-dlp invocations.

Gotcha that cost real time: **`ChildStdin::shutdown()` does NOT close the
pipe** — ffmpeg never saw EOF, never flushed, and the response hung forever.
The stdin handle must be dropped. (Symptom: integration suite deadlocked.)

Deviations: the plan's "continue download if ≥20% transferred" needs a total
size yt-dlp doesn't provide for `-o -`; an absolute 256 KiB floor
approximates the intent. ytsearch flat-playlist used instead of full
extraction (one yt-dlp call instead of six, ~5× faster resolve).

## M4 — Ingest (2026-06-10)

Done: background worker (5s poll, 1s in tests) claims `finalizing` jobs:
ffprobe → remux with `-c:a copy` for opus/vorbis/aac/alac/mp3/flac (unknown
codecs transcode to opus once), lofty tags (artist/title/album/ISRC +
`COMMENT=songarr:src=<url>`), move to
`/music/<ingest_subdir>/<Artist>/<Artist> - <Title>.<ext>` (sanitized,
collision → 6-char suffix), `startScan` + poll `getScanStatus`, resolve the
real id via admin `search3` (normalized match, prefer paths under the ingest
subdir), set `real_subsonic_id`, replay pending scrobbles/stars with the
captured user tokens (failures logged + dropped), append to the optional
rolling m3u. Score below `min_score_for_import` (or `auto_import=false`) →
`needs_review`, file kept in staging; `ingest_job_forced` exists for the M5
approve button. Post-import, `stream`/`getSong`/`getCoverArt` on the old
virtual id rewrite to the real id and pass through (seeking works), and
search dedup prefers the real track.

Verified: 32 unit tests; ingest suite — full end-to-end (scrobble queued
before play is replayed: Navidrome playCount ≥ 1; virtual-id stream now
returns Content-Length; no sgr_ duplicate in fresh searches; playlist line
written) and the low-score park (streams fine, never imports, staging file
kept, reason recorded). All four integration suites green together.

Next: M5 — admin mini-UI (jobs list, needs_review approve/reject, per-user
toggles, /metrics).

## Latency: speculative resolution prefetch (2026-06-10)

Real-world testing (Supersonic on macOS) showed 4–8s click-to-audio: yt-dlp
search (~2–4s) + yt-dlp extract/download start (~1.5–3s), serial. Fix:
migration 0002 adds a per-track resolution cache (url/score/title/epoch,
24h TTL); after search injection the proxy speculatively resolves the top 4
injected results in the background (deduped via an in-flight set — clients
search per keystroke — and bounded by a 2-permit semaphore). Pressing play
hits the cache and skips the search entirely → typical first-audio ~1.5–3s.
Also `-flush_packets 1` on the live ffmpeg pipe (ogg pages were buffered
~1s). Test proves a cached play runs zero additional yt-dlp searches.
Test proxies now always default to the mock yt-dlp so prefetch can never
reach real YouTube from the suites.

Field notes from first real-client session (Supersonic):
- `0.0.0.0` bind is IPv4-only; macOS resolves `localhost` → `::1` first and
  Supersonic doesn't fall back → "could not reach server". Workaround:
  connect via `127.0.0.1`. TODO(M7): dual-stack listener.
- Navidrome web UI search uses the native `/api`, NOT subsonic `search3` —
  virtual results are structurally invisible there. Documented; real
  Subsonic clients (incl. Supersonic) work as designed.

## Innertube post-mortem: PO-token wall; yt-dlp manifest-skip (2026-06-11)

Live-probing googlevideo showed the innertube direct path never worked: the
iOS-client URLs 403 any HTTP `Range:` header AND any plain GET — bytes are
served only via a `&range=start-end` QUERY PARAM — and the old code sent an
8 MiB header range, so the very first chunk 403'd and every play silently
paid a wasted round-trip before falling back to yt-dlp. Fixed the protocol
(query-param ranging, 1 MiB chunks, loop driven by the format's
`contentLength` since responses are 200 without Content-Range), then hit the
real wall: without a PO token the server only serves ranges ending below
~1.05 MiB (binary-searched: last allowed end byte 1097007), regardless of
range start. That's the tokenless free window — full songs need the
bgutils/JS-challenge dance yt-dlp does. So `open_media_stream` now validates
2 MiB INLINE before committing (a token wall → clean error → whole-song
yt-dlp fallback, never a mid-stream truncation), and `innertube` defaults to
OFF; enable only on non-token-gated (residential) egress. Mock youtube
updated to the measured contract: no/oversized `range` param → 403, slices
otherwise, `contentLength` advertised.

The latency win moved to the pipe we actually use: yt-dlp spent ~1s of its
~2.5s ttfb fetching HLS/DASH manifests that `bestaudio` never needs.
`--extractor-args youtube:skip=hls,dash,translated_subs` (+ `--no-playlist`)
→ ttfb 2.4–2.7s → 1.4–1.7s, full file byte-identical. Single-client
extraction (`player_client=ios`/`web`/...) was a dead end: only
tv_embedded/web_embedded still stream and they're no faster.

Verified: 34 unit; all four integration suites green (incl. innertube direct
path against the strict mock, fallback, and proxy-arg passthrough).
