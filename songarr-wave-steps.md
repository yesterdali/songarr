# Songarr Wave PWA — Step-by-step build program

The execution checklist for the PWA. Architecture, auth, and the wave-API
design live in `songarr-wave-plan.md`; this file is the **order we build in**
and how we know each step is done. We do them one at a time and don't start
the next until the current one runs on the real target phone.

Guiding rules:
- **Value first.** The Wave button is the thing no free client has; it ships
  before library/search/playlist chrome.
- **Every step is usable on its own.** You can stop after any step and have a
  working (if smaller) app. Nothing early depends on something later.
- **Verify on the phone, not just localhost.** Especially audio + iOS Safari.
- Reuse the backend that already exists (recs engine, virtual tracks,
  `/rest/*`, artist expansion). The PWA is a face, not a new system.

Legend: [ ] todo · [~] in progress · [x] done

---

## Phase 0 — De-risk and foundation

### Step 1 — [~] Spike: does our stream play in a phone browser?  (throwaway)
The single biggest unknown. Serve one static HTML page from songarr at
`/wave/` with a hardcoded `<audio src="/rest/stream?id=<a real sgr_ track>&…auth…">`
and a play button.
- **Verify (on the friend's phone, not the laptop):** the track plays; it
  keeps playing with the screen locked; iOS Safari doesn't choke. Add a
  minimal Media Session call and confirm lock-screen controls appear.
- **Why first:** if background audio is unreliable on the target device, that
  changes the whole approach — better to know now than after building the UI.
- Throwaway code; delete once proven.
- **Implementation:** `/wave/spike?id=sgr_...&u=...&t=...&s=...` serves the
  audio probe page. Still needs the real phone/background-audio verification.

### Step 2 — [x] Serve the real React build from songarr at `/wave/`
Embed `web/dist` into the binary (`rust-embed`) and serve it under `/wave/`,
with a SPA fallback (unknown sub-paths → `index.html`), registered **before**
the Navidrome passthrough fallback.
- **Verify:** `cargo build` produces one binary; opening `http://<host>:4534/wave/`
  on the phone shows the scaffolded screen; `/rest/*` still works.
- **Why here:** gets the actual app onto the phone early, so every later step
  is testable in the real environment, not just `npm run dev`.

### Step 3 — [x] Login + token auth + session
Login screen: username + password → validate with `/rest/ping` → compute
`token = md5(password + salt)`, store `{username, token, salt}`, discard the
password. All later requests append the auth params.
- **Verify:** wrong password is rejected; right one persists across reload and
  app restart; logout clears it.
- **Why here:** everything past this needs an authenticated session.

---

## Phase 1 — Wave core (the unique value)

### Step 4 — [x] `/wave/api/next` (cold start) + finite playback
Server endpoint returns a chunk of ready-to-play tracks (cold-start sourcing:
recent listens → Discovery → never-empty fallback). Client plays them through
a single `<audio>`, advancing on end/skip; now-playing bar with
play/pause/skip.
- **Verify:** tap «Твоя волна» → hear personalized music → skip through the
  chunk. The hero button finally does something.
- **Implementation:** `/wave/api/next` returns playable tracks and the hero
  starts a finite Wave queue through the shared player.

### Step 5 — [~] Endless + smooth
Auto-extend: when ≤2 tracks remain, fetch `/wave/api/next?seedId=<current>`
and append. Preload the next track. Full Media Session (metadata + lock-screen
play/pause/next).
- **Verify:** leave it playing for an hour — it never stops; transitions don't
  stall; lock-screen controls work.
- **Implementation:** auto-extend and Media Session are wired. Explicit
  next-track preloading still needs a device pass.

### Step 6 — [x] Adaptive (like / skip / dislike)
Migration `0005_wave_feedback`; `POST /wave/api/feedback`; wire ♥ like, skip,
and dislike. `next` honors signals: seed from likes, cool down skips, suppress
dislikes. `play`-through also feeds the existing listens/scrobble path.
- **Verify:** disliking an artist stops it recurring; liking pulls more like
  it. **This is the Yandex "Моя волна" feel — the headline milestone.**
- **Implementation:** `wave_feedback` stores play/skip/like/dislike; `next`
  seeds from positive feedback and suppresses skipped/disliked tracks/artists.

> At this point the unique feature is fully done. Everything below turns the
> Wave app into a general player. Optional — build as far as you want.

---

## Phase 2 — Make it a real player

### Step 7 — [x] Likes / favorites
Star/unstar via `/rest/star` + `/rest/unstar`; a "Liked" list via
`getStarred2`. Reuse the ♥ from step 6.
- **Verify:** liking in Wave shows up in the Liked list and in other Subsonic
  clients (it's real Navidrome state).
- **Implementation:** hearts call `star`/`unstar`; the Liked view reads
  `getStarred2`.

### Step 8 — [x] Search
Search box → `/rest/search3` → results (songs, and the virtual ones songarr
injects) → tap to play or queue.
- **Verify:** searching a song not in the library returns a playable virtual
  result that streams.
- **Implementation:** Search tab uses `search3` and playable result rows.

### Step 9 — [x] Library + artist browse
Browse artists/albums: `getArtists`, `getAlbumList2`, `getAlbum`. Artist pages
get the external albums/top-tracks from the artist-expansion work for free.
- **Verify:** open an artist → see local + Songarr external albums → open one
  → play a track.
- **Implementation:** Library tab browses artists; artist and album routes use
  `getArtist`/`getAlbum`.

### Step 10 — [x] Playlists
`getPlaylists` / `getPlaylist`, including the synthetic "Songarr Discovery".
Play a playlist; (optional) create/edit via `createPlaylist`/`updatePlaylist`.
- **Verify:** Discovery and normal playlists list and play.
- **Implementation:** Playlists tab uses `getPlaylists`/`getPlaylist`.

### Step 11 — [~] Queue / now-playing screen
Full-screen now-playing with the upcoming queue; reorder, add-next, recently
played.
- **Verify:** queue survives navigation; reordering works mid-playback.
- **Implementation:** full-screen now-playing and up-next list exist; queue
  reordering is still missing.

---

## Phase 3 — Polish and ship

### Step 12 — [~] PWA install + states
Finalize manifest + real icons; service worker caches the shell only (never
audio or `/wave/api`); light/dark; loading/empty/error/offline states.
- **Verify:** "Add to Home Screen" on Android Chrome **and** iOS Safari;
  launches standalone; survives a mid-wave network blip.
- **Implementation:** manifest/service worker are configured for shell
  caching. Offline/network-blip states still need a device pass.

### Step 13 — [~] Single-binary delivery + device matrix
Wire the embedded assets into the Docker image; one `cargo build` serves API +
PWA. Final pass on Android Chrome and iOS Safari.
- **Verify:** fresh `docker run` serves the installable app; both devices play
  background audio with lock-screen controls.
- **Implementation:** `cargo build` embeds `web/dist` into the binary. Docker
  wiring and the device matrix remain.

---

## Stop-anytime map

- After **Step 6**: the friend has the endless adaptive Wave button. ✅ core goal
- After **Step 9**: a browsable music app with Wave + search + artists.
- After **Step 13**: a self-contained, installable, full-ish client.
