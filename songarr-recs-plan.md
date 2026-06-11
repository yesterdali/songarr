# Songarr Recommendations — Implementation Plan (v1)

Companion to `songarr-proxy-plan.md`. Builds on the M1–M4 foundation:
anything that can produce `(artist, title)` pairs becomes playable via the
existing virtual-track machinery, so recommendations are "just" another
producer of virtual tracks — like search injection, but seeded by a track
or a user's listening history instead of a typed query.

## 1. What we are building

An ensemble recommender behind the Subsonic endpoints clients already have
UI for. Multiple sources vote; no single source is load-bearing.

- **Song radio**: `getSimilarSongs` / `getSimilarSongs2` return a mix of
  similar tracks (virtual + already-imported real ones). Clients with a
  "radio"/"more like this" button (Symfonium, DSub, Tempo; verify
  Supersonic) get infinite playback where each track materializes on play.
- **Artist top songs**: `getTopSongs` filled from external sources for
  artists with little/no local presence.
- **Discovery playlists**: per-user synthetic playlists ("Songarr Discovery")
  served by intercepting `getPlaylists`/`getPlaylist` — works in every
  client, no special UI needed. Seeded from the listening history the proxy
  already observes at the scrobble chokepoint.

### Sources (the ensemble)

| Provider | Auth | Strength | Failure mode |
|---|---|---|---|
| YTM innertube `next` | none | freshness, breadth, strong RU | schema rotation (contained, like `innertube.rs`) |
| Last.fm `track.getSimilar` / `artist.getSimilar` | free API key | similarity graph, long tail | rate limits |
| Deezer `artist/{id}/related` + `top` | none (already integrated) | clean metadata, ISRC | weak for new releases |
| ListenBrainz | none (token optional) | personalized CF from our own scrobbles | cold start |
| VK audio (optional, R4) | user token, unofficial | best for new Russian music | ToS-gray, breaks often — never required |

### Explicit non-goals for v1

- No ML/embedding work of our own — we aggregate other people's models.
- No per-user collaborative filtering beyond what ListenBrainz returns.
- No recommendation UI of our own (the admin mini-UI is M5's problem);
  everything surfaces through standard Subsonic endpoints.
- VK is a bonus voter for personal use behind explicit config, default off.
  It must never be a dependency of any feature.

## 2. Critical protocol knowledge

- `getSimilarSongs2?id=X&count=N`: `id` may be a real Navidrome id OR our
  virtual `sgr_` id. Response key `similarSongs2` with `song` children —
  same `SongEntry` shape search injection already emits.
- `getTopSongs?artist=NAME&count=N`: keyed by artist *name*, key `topSongs`.
- Navidrome implements `getSimilarSongs*` itself when Last.fm is configured
  there; our interception REPLACES it (we still merge Navidrome's own
  response body when non-empty — its entries count as one voter and keep
  local real tracks ranked first).
- YTM radio: `POST /youtubei/v1/next` with `videoId` + iOS client context
  (same headers as `resolve/innertube.rs`) returns a watch-queue of ~20
  entries with `videoId`, title, artist/byline, duration. **Candidates
  arrive pre-resolved** — store the videoId as `resolved_url` at upsert so
  pressing play skips search resolution entirely (instant start, like the
  prefetch cache).
- Last.fm: `track.getSimilar` wants artist+title (good — that's all we
  have); returns `match` score 0..1 we can fold into ranking.
- Seed mapping: virtual id → `vtrack::get` for artist/title; real id →
  `getSong` against Navidrome (admin creds, same as ingest verification).

## 3. Data model (SQLite, migration 0003)

```sql
-- every play the proxy observes, real or virtual: the personalization fuel
CREATE TABLE listens (
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  subsonic_id TEXT,             -- real or sgr_ id at scrobble time
  listened_at_epoch INTEGER NOT NULL
);
CREATE INDEX idx_listens_user_time ON listens(username, listened_at_epoch);

-- per-(source, seed) response cache; similarity changes slowly
CREATE TABLE rec_cache (
  source TEXT NOT NULL,         -- ytm | lastfm | deezer | listenbrainz | vk
  seed_key TEXT NOT NULL,       -- normalized SongKey or artist key
  payload_json TEXT NOT NULL,   -- Vec<Candidate>
  fetched_at_epoch INTEGER NOT NULL,
  PRIMARY KEY (source, seed_key)
);

-- anti-repetition: what we already showed each user
CREATE TABLE rec_shown (
  username TEXT NOT NULL,
  song_key TEXT NOT NULL,       -- normalized artist|title
  shown_at_epoch INTEGER NOT NULL,
  PRIMARY KEY (username, song_key)
);
```

`virtual_tracks` is unchanged: YTM candidates upsert with
`provider='ytmusic'`, `provider_track_id=videoId` (the UNIQUE constraint
and `sgr_` id machinery already handle it).

## 4. Architecture

```
seed (track | artist | user history)
  │  fan out concurrently, per-provider timeout ~2s, errors = skip
  ├─► recs/ytm.rs         (innertube next)
  ├─► recs/lastfm.rs      (similar track/artist)
  ├─► recs/deezer.rs      (related artists → top tracks)
  ├─► recs/listenbrainz.rs
  └─► recs/vk.rs          (R4, feature-flagged)
        │ all return Vec<Candidate> { artist, title, duration_ms?,
        │                             source_rank, video_id?, deezer_id? }
        ▼
  recs/merge.rs: normalize → SongKey (reuse search.rs normalization)
    score = Σ over voting sources (source_weight × rank_decay(source_rank))
    multi-source votes dominate; interleave so one source can't flood
        ▼
  filter: drop already-imported / in-library (existing SongKey dedup),
          drop rec_shown within cooldown (default 7 days)
        ▼
  canonicalize top-K via Deezer search (ISRC, artwork, duration);
  miss → keep source metadata (YTM thumbnail for artwork)
        ▼
  vtrack::upsert → SongEntry::from_virtual → inject into response
```

Module layout: `src/recs/mod.rs` (Candidate, aggregator, config dispatch),
one file per provider, `src/proxy/similar.rs` (endpoint handlers). Routing
via the existing `intercept!` macro in `lib.rs`.

Philosophy carried over from streaming: **degrade quality, never
availability**. Any provider error → that source contributes nothing this
request. All providers failing → fall back to passthrough (Navidrome's own
response), exactly like search injection does on catalog failure.

### config.example.toml addition

```toml
[recommendations]
enabled = true
max_results = 20
shown_cooldown_days = 7
cache_ttl_hours = 72
# weight 0 disables a source; voting needs >=2 enabled to be meaningful
weight_ytm = 1.0
weight_deezer = 0.6
weight_lastfm = 0.8        # requires lastfm_api_key
weight_listenbrainz = 0.8
weight_vk = 0.0            # unofficial API — read R4 caveats before enabling
lastfm_api_key = ""
listenbrainz_token = ""    # optional; enables listen submission
vk_token = ""
```

## 5. Milestones

### R1 — Song radio via YTM (prove the loop end-to-end)

The thinnest vertical slice: one provider, no new auth, both endpoints.

- Migration 0003; `recs/` skeleton; `Candidate`; config section.
- `recs/ytm.rs`: seed track → videoId (resolution cache hit, else the
  existing resolve path) → `next` → parse queue. Same fragility containment
  as `innertube.rs`: any parse failure → empty vec + debug log.
- `src/proxy/similar.rs`: `getSimilarSongs`/`getSimilarSongs2`/`getTopSongs`
  handlers — seed mapping, merge with Navidrome's own response, filter,
  upsert (with pre-resolved videoId), inject, JSON+XML.
- Tests: mock `next` on the existing `spawn_mock_youtube`; unit tests for
  queue parsing + injection; integration: radio for a virtual seed returns
  virtual songs, playing one streams (reuses M3 harness); fallback test
  (mock down → Navidrome passthrough body, status 200).
- Exit: friend-shaped demo — play one Russian track, hit song radio, get a
  playable RU mix.

### R2 — The ensemble (voting, weights, cache, anti-repeat)

- `recs/lastfm.rs`, `recs/deezer.rs` (related→top, reuse `catalog::deezer`
  plumbing); `merge.rs` voting/interleave; `rec_cache` + `rec_shown`.
- Canonicalization pass via Deezer for chosen candidates.
- Cyrillic/transliteration hardening of SongKey matching (e.g. Скриптонит
  vs Skryptonite): ISRC match when both sides have one; else duration
  tolerance + a small RU translit table in the normalizer. Unit-test with
  real RU artist fixtures.
- Tests: merge scoring is pure-function unit-testable (multi-source vote
  beats single high rank; weights respected; interleaving). Integration:
  two mock sources, overlapping candidate wins.

### R3 — Personalization (listens → discovery playlist)

- Log listens in `scrobble.rs` for BOTH branches (virtual stores already
  happen; add the real-id passthrough branch).
- `recs/listenbrainz.rs`: submit listens (token set) + fetch CF recs.
- Weekly per-user seed set: top recent artists/tracks from `listens` →
  ensemble → "Songarr Discovery" synthetic playlist; intercept
  `getPlaylists`/`getPlaylist(.view)` to append/serve it (virtual ids never
  touch Navidrome's playlist store). Regenerate lazily on fetch when stale
  (>6 days) — no scheduler needed.
- Tests: listens accumulate from both id kinds; playlist appears, is
  stable within the week, contains no library duplicates.

### R4 (optional) — VK voter

- `recs/vk.rs` behind `vk_token` + nonzero weight, default off:
  `audio.getRecommendations` via the reverse-engineered mobile-client auth.
  Isolated like `innertube.rs`, expected to break; breakage = one silent
  voter gone. Documented as personal-use, ToS-gray, never load-bearing.
- Exit: with the friend's own token, his VK signal joins the vote.

## 6. Testing strategy

Same doctrine as the main plan: providers are mocked axum servers in
`tests/common/mod.rs` (the `spawn_mock_youtube` pattern), merge logic is
pure and unit-tested, integration suites run against the harness Navidrome
and MUST tolerate provider absence. No test may ever hit a real external
service. New suite: `tests/recommendations.rs`.

## 7. Definition of done for v1

- Song radio works in at least two real clients against a virtual AND a
  real seed; every recommended track is playable (materializes on play).
- Killing any single provider changes ranking, never availability; killing
  all providers yields Navidrome's own response, status 200.
- A week of listening produces a Discovery playlist that excludes the
  library, excludes recent repeats, and survives proxy restart.
- RU-content matching: the Cyrillic/Latin fixture suite passes (no
  duplicate Скриптонит/Skryptonite pairs in one response).
