# Songarr → Discord remote control ("Connect to Vivaldi")

## Context & goal
Make the Songarr web/desktop app a **remote control** for the Discord bot
(Vivaldi), Spotify-Connect style. When you "Connect to Discord":

- Audio plays **only in your Discord voice channel** — the app does **not** play
  locally while connected.
- The app's playbar becomes a remote: play/pause, next/prev, seek, "play this
  track/album/playlist", "start recs/wave", and the queue all drive the bot.
- The playbar reflects **the bot's** state (current track, progress, play/pause).

This is the confirmed model: one player (the bot), the app is a controller — no
mirrored/duplicate playback to keep in sync.

## The core constraint (shapes the whole design)
The bot has **no inbound HTTP server**; it only makes outbound calls (to Discord
and to Songarr). So **Songarr is the relay** between app and bot:

```
app  ──(write command)──►  Songarr  ──(bot polls/executes)──►  bot → Discord voice
app  ◄──(render state)──   Songarr  ◄──(bot writes state)───   bot
```

Identity is already solved: `/link` ties `discord_id ↔ songarr account`, so the
bot knows whose commands to run and whose voice channel to join. The bot already
authenticates per-user with that token and already streams tracks from Songarr
(the `/play` path) — remote control just automates and extends that.

## Data model (new Songarr migration)
- `remote_command(username, seq INTEGER, action TEXT, payload TEXT /*json*/, created_at_epoch)`
  — append-only per-user command log. The bot consumes in `seq` order and tracks
  the last `seq` it ran (so two quick skips both register).
- `remote_state(username PRIMARY KEY, connected INTEGER, track_id, title, artist,
  album, cover_art, position_ms, duration_ms, is_playing, queue_json,
  updated_at_epoch)` — the bot's reported state. This is `user_activity`'s
  now-playing generalized with position / is_playing / queue + a heartbeat
  (`updated_at` doubles as "device alive"). Friend Activity can later read from
  here.

## Endpoints (Songarr proxy, mirror existing `/wave/api/*` auth)
- `POST /wave/api/remote/command` — app enqueues `{ action, payload }`.
  Actions: `connect`, `disconnect`, `play {ids[], startIndex}`, `wave`,
  `pause`, `resume`, `next`, `prev`, `seek {positionMs}` (`setVolume` later).
- `GET  /wave/api/remote/commands?after=<seq>` — bot pulls new commands (its own user).
- `POST /wave/api/remote/state` — bot reports state + heartbeat.
- `GET  /wave/api/remote/state` — app reads the bot's state to render the playbar.

All auth via the existing Subsonic creds (bot uses the linked token; app uses the
session) and the existing `authenticated()` helper.

## Bot changes
A per-user **remote control loop** (started on `connect`):
- On `connect`: find the user's current voice channel (bot has
  `GUILD_VOICE_STATES`), join it, begin heartbeating state.
- Poll `GET /wave/api/remote/commands?after=last_seq` ≈ every 1s; execute each:
  - `play {ids}` → resolve stream URLs (reuse `songarr::stream_url`), clear the
    queue, enqueue (songbird `builtin-queue`).
  - `wave` → start endless wave (reuse `WaveRefiller`).
  - `pause`/`resume` → `TrackHandle.pause()/play()`.
  - `next` → `queue.skip()`. `prev` → **needs a history stack** (songbird has no
    native previous): keep recently-played handles/labels and re-enqueue.
  - `seek {positionMs}` → `TrackHandle.seek(Duration)`.
- After each action and on a ~1s timer, `POST /wave/api/remote/state` with
  current track (from `LabelMap`), `position` (`TrackHandle.get_info().position`),
  `is_playing`, and the queue.
- On `disconnect` (or empty channel / idle timeout): leave voice, set
  `connected=false`.

New bot work: a background task per linked user, a prev-history stack, and the
state reporter. The current bot is purely slash-command driven, so the loop is
the main new piece.

## Web changes
- **"Connect to Discord" toggle** in the left sidebar (desktop) showing device
  state — e.g. "Играет в Discord (Vivaldi)" with a green dot when connected. On
  mobile, surface it in the now-playing screen.
- **Remote mode in the player** (`player.tsx`):
  - When connected: do **not** load the local `<audio>`; suppress local playback
    (frees the Songarr stream slot too).
  - Player state (current, isPlaying, position, duration, queue) is sourced from
    `GET /wave/api/remote/state` (poll ~1–2s) with **client-side progress
    interpolation** (advance position from the reported timestamp) so the seekbar
    moves smoothly between polls.
  - Transport + "play album/track/recs" dispatch **commands** instead of touching
    local audio, with **optimistic UI** (reflect the action immediately, reconcile
    on the next state poll).
  - Indicator + "disconnect" affordance; if the heartbeat goes stale (bot gone),
    show "disconnected" and fall back to local playback.

## Phasing
- **Phase 1 — core experience:** connect/disconnect, `play` (track/album/queue),
  pause/resume, next, seek; bot state heartbeat; app remote mode + playbar binding
  + optimistic UI. Delivers the feature end to end.
- **Phase 2 — parity:** prev (history), recs/wave from the remote, queue-view
  sync, robust reconnect/expiry, multi-guild voice-channel selection.
- **Phase 3 — snappy (optional):** replace polling with **SSE/WebSocket push**
  (Songarr→app for state, Songarr→bot for commands) to cut control latency from
  ~1–2s to near-instant. Only if the lag bugs you in practice.

## Tradeoffs / caveats
- **Latency:** polling ⇒ ~1–2s control lag; masked by optimistic UI; Phase 3 fixes
  it properly.
- **Preconditions:** you must have `/link`'d and be in a voice channel — the toggle
  warns otherwise.
- **prev/seek** ride on songbird capabilities (seek ✓; prev needs the history stack
  we add).
- **Single active device** per user: connecting from the app takes over.
- **Geo:** unchanged — the bot streams from Songarr (RU) exactly as `/play` does.

## Verification
- Unit: command (de)serialization + `seq` consumption ordering.
- Local (no Discord): drive the relay with `curl` — POST a `play` command, GET
  commands as the bot, POST/GET state — to prove the relay + the app's remote mode
  without voice.
- Live: bot on the VPS — connect from the app, confirm it joins voice and plays,
  that skip/seek/pause/recs from the app control it within ~1–2s, and that the
  app's playbar tracks the bot's state.
