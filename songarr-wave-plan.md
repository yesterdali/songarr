# Songarr Wave (Твоя волна) — Implementation Plan (v1)

Companion to `songarr-proxy-plan.md` and `songarr-recs-plan.md`.

The goal: the Yandex "Моя волна" experience — one big button that starts an
**endless, personalized, adaptive** music stream — for free, self-hosted, on
any phone, with no proprietary client. A small React PWA served by songarr
itself, sitting on top of the recommendation engine that already exists.

## 1. Why a PWA (not a forked client)

The button doesn't exist in Feishin/Supersonic, so either path means *writing*
it. The PWA writes only the feature; a fork writes the feature **plus**
inherits a foreign codebase, its release treadmill, and binary distribution.
songarr is already an HTTP server with recommendations, virtual-track
materialization, and `/rest/stream`; the browser supplies the player
(`<audio>`), lock-screen UI (Media Session API), and installability
(manifest + service worker). What's left is genuinely small.

Non-goals for v1: a full library browser, search, settings screens, offline
audio download, gapless playback. Wave is one screen that does one thing well.

## 2. Architecture

```text
  Phone browser (installed PWA)
    React + Tailwind single screen
      <audio> ── plays ──► /rest/stream?id=sgr_…&u=&t=&s=   (exists)
      fetch ──── queue ──► /wave/api/next                    (new, thin)
      fetch ──── signals ─► /wave/api/feedback               (new, thin)
        │
  songarr (axum)
    /wave/            → serve embedded PWA assets (new route, before fallback)
    /wave/api/*       → JSON, reuses recs engine + listens + feedback
    /rest/*           → existing Subsonic interception + passthrough
```

**Same-origin by design.** The PWA is served from songarr at `/wave/`, so
calls to `/rest/*` and `/wave/api/*` are same-origin — no CORS, no token
juggling across hosts. In dev, the Vite server proxies those paths to a
running songarr, preserving the same-origin model.

**Serving the assets.** Dev: `npm run dev` (Vite, HMR) proxying `/rest` and
`/wave/api` to the dev songarr (`127.0.0.1:4534`). Prod: `npm run build` →
`web/dist`, embedded into the binary with `rust-embed` and served under
`/wave/` — keeps songarr a single self-contained binary, consistent with the
Docker story. The `/wave/` route and a SPA fallback (serve `index.html` for
unknown sub-paths) must be registered **before** the Navidrome passthrough
fallback in `build_app`.

## 3. Auth

The PWA authenticates as a normal Subsonic client using the user's Navidrome
credentials (the same ones they'd type into Feishin):

- Login screen takes username + password, validates with `/rest/ping`.
- Compute Subsonic token client-side: `salt` random, `token = md5(password +
  salt)`. Store `{username, token, salt}` in `localStorage`; **discard the
  password**. (Web Crypto has no MD5, so bundle a ~1 KB md5 helper.)
- Every request appends `u, t, s, v=1.16.1, c=wave` (and `f=json` for REST).
- `/wave/api/*` accepts the same query auth and resolves the username from it.

Multi-user falls out for free: the wave is per-username, seeded from that
user's own listens/feedback.

## 4. Server: the Wave API (new, thin)

Two JSON endpoints, both reusing the recommendation engine. Keeping the
queue/dedup/feedback logic server-side keeps the PWA thin and lets any future
client reuse it.

### `GET /wave/api/next?count=N&seedId=<optional>`

Returns the next chunk of the wave as ready-to-play items:

```json
{ "tracks": [
  { "id": "sgr_…", "title": "…", "artist": "…", "album": "…",
    "durationSecs": 195, "coverArt": "sgr_…",
    "streamUrl": "/rest/stream?id=sgr_…&…auth…" }
] }
```

Sourcing:
- `seedId` present (the currently-playing track) → extend the wave:
  `recommended_for_seed` on that track (this is what auto-extend calls).
- No seed (cold start) → seed from the user's recent positive signals
  (`listens` + likes), falling back to the Discovery engine, falling back to
  `getRandomSongs` so the wave is never empty for a brand-new user.
- Server-side dedup against the recent queue (a short per-user ring in
  `rec_shown`) and the `shown_cooldown`, so the wave doesn't loop.

### `POST /wave/api/feedback`

```json
{ "trackId": "sgr_…", "action": "play" | "skip" | "like" | "dislike" }
```

Records the signal (migration below) and shapes future `next`:
- `like` / `play` (played through) → positive: boost that artist/track as a seed.
- `skip` (early) → mild negative: cool down that artist for a while.
- `dislike` → strong negative: suppress artist/track for a long cooldown.

`play` also feeds the existing scrobble/listens path so the wave and the
Discovery playlist share one taste profile.

### Migration `0004_wave_feedback.sql`

```sql
CREATE TABLE wave_feedback (
  username TEXT NOT NULL,
  song_key TEXT NOT NULL,        -- normalized artist|title (recs::song_key)
  artist_key TEXT NOT NULL,      -- normalized artist (recs::artist_key)
  action TEXT NOT NULL,          -- play | skip | like | dislike
  created_at_epoch INTEGER NOT NULL,
  PRIMARY KEY (username, song_key, action)
);
CREATE INDEX idx_wave_feedback_user_time ON wave_feedback(username, created_at_epoch);
```

## 5. Frontend

Stack: React + TypeScript + Tailwind (v4) + Vite, `vite-plugin-pwa` for the
manifest + service worker.

### Screen (mirrors the reference UI)

- **Hero card**: gradient, big circular play button, "Твоя волна" + subtitle.
  Tap → start/resume the wave.
- **Now-playing bar**: cover, title, artist, ♥ like, play/pause, skip.
- **Up-next peek** (optional): the next 1–2 queued tracks.
- Light/dark, responsive, phone-first.

### Playback engine (the core client logic)

- Single `<audio>`; on `ended` or skip, advance the queue.
- **Auto-extend**: when ≤2 tracks remain, `fetch /wave/api/next?seedId=<current>`
  and append. The wave never ends.
- **Preload**: warm the next track's `<audio>` (or a second element) so
  transitions don't stall on resolve/transcode.
- **Media Session API**: title/artist/artwork + play/pause/next handlers, so
  lock-screen and headset controls work.
- **Feedback hooks**: play-through → `play`; skip button → `skip`; ♥ → `like`;
  long-press/✕ → `dislike`. Fire-and-forget POSTs.

### Service worker

Cache the app shell (HTML/JS/CSS/icons) for installability and instant load.
**Never cache audio or `/wave/api` responses** — those must be live.

## 6. Milestones (ordered by risk — unknowns first)

### W0 — Spike: can a phone browser play our stream? (de-risk before UI)

Throwaway. Serve one static HTML page from songarr with a hardcoded
`<audio src="/rest/stream?id=…&auth…">` and a play button. Open it in the
friend's phone browser. Confirm: auth works from a browser, the stream plays,
playback survives screen-lock (Media Session), iOS Safari behaves. This is the
single biggest unknown; everything else is downstream of it.

Exit: a real recommended track plays on the target phone, screen locked.

### W1 — App shell + login + finite wave

Vite/React/Tailwind scaffold; login → token; hero button starts playback of an
initial `/wave/api/next` chunk (cold-start path); now-playing bar with
play/pause/skip over a fixed queue.

Exit: tap the button, hear personalized music, skip through the queue.

### W2 — Endless + smooth

Auto-extend when the queue runs low; preload next; Media Session controls.

Exit: leave it playing for an hour; it never stops and transitions are clean.

### W3 — Adaptive

`wave_feedback` migration + endpoint; like/skip/dislike wired; `next` honors
the signals (seed from likes, cool down skips/dislikes).

Exit: disliking an artist visibly stops it recurring; liking pulls more like it.

### W4 — Installable PWA + polish

Manifest (name "Songarr — Твоя волна", icons, standalone, theme color),
service worker for the shell, light/dark, empty/error/loading states. Device
pass: Android Chrome + iOS Safari (install to home screen, background audio).

### W5 — Single-binary delivery

`rust-embed` the built `web/dist` under `/wave/` with SPA fallback; wire into
the Docker image; `cargo build` produces one binary that serves API + PWA.

## 7. Testing

- **Server**: `/wave/api/next` and `/feedback` get integration tests in the
  existing harness style (mock Navidrome/YTM, no real services) —
  cold-start/seeded/never-empty for `next`; feedback shapes subsequent `next`.
- **Client**: keep light. Vitest over the pure logic (queue refill threshold,
  auth-token build, dedup) — not the React tree. No e2e harness in v1.
- **Manual device matrix**: Android Chrome, iOS Safari — install, background
  audio, lock-screen controls, network blips mid-wave.

## 8. Open questions

- iOS Safari background audio in an installed PWA is historically the weakest
  spot — test in W0, not W4. If it's unreliable, document "keep the tab
  foregrounded on iOS" rather than block the feature.
- Embed assets (`rust-embed`, one binary) vs serve from a directory
  (`tower-http ServeDir`, simpler dev, two artifacts to ship)? Leaning embed
  for deploy parity; revisit if build ergonomics bite.
- How aggressive should skip-cooldown be? Too soft and the wave ignores you;
  too hard and it starves. Start gentle, tune against real listening.
- Should the wave honor time-of-day / "more familiar vs more new" sliders like
  Yandex? Out of scope for v1; the feedback schema leaves room.
```
