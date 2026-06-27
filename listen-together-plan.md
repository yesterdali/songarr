# Listen Together + Personalization — plan

## Goals
- **Personalization:** a per-user **display name + avatar**, decoupled from the
  Navidrome login, shown everywhere people appear (sidebar, friend activity,
  listen rooms).
- **Listen Together:** synced group listening — friends each play the **same
  track at the same position on their own devices**, collaborative control,
  joined via a **room code/link**. Self-hosted, source-agnostic (library +
  YouTube/Yandex/VK).

**Decisions baked in:** collaborative control (anyone in the room can drive);
join via room code/link (a friends-list invite flow can come later); sessions
are ephemeral. The actual Navidrome **login username stays the auth identity** —
"changing your username" means your **display name**, which is what others see.

This is ~70% assembly: the relay (per-user command queue + shared state +
long-poll/notify), the room/claim/control model, and client-side position
interpolation already exist from the Discord remote. The one genuinely new
problem is **clock sync** for audio.

---

## Phase A — Personalization (foundation)

### Data (proxy, migration 0011)
`user_profile(username PRIMARY KEY, display_name TEXT, avatar_blob BLOB,
avatar_mime TEXT, updated_at_epoch INTEGER)`. Avatars are small (client resizes
to ≤256px, ≤256 KB) so a DB blob is fine; alternative is `/config/avatars/`.

### Endpoints (auth = existing Subsonic creds)
- `GET  /wave/api/profile` → caller's `{ displayName, hasAvatar }`.
- `PUT  /wave/api/profile` `{ displayName }`.
- `PUT  /wave/api/profile/avatar` (raw image body) → store blob + mime.
- `DELETE /wave/api/profile/avatar`.
- `GET  /wave/api/avatar?user=<name>` → that user's avatar image, or 404 (client
  falls back to the initial). The caller is authed; any user's avatar is
  readable (same as cover art).
- Helper `resolve_profile(username) -> { display_name||username, has_avatar }`,
  batchable for friend/member lists.

### Integrations
- **Friend activity** (`getFriends`): add `displayName` + `avatarUrl` per friend.
- **Sidebar account card:** show display name + avatar.
- **New Settings screen** (web): edit display name, upload/remove avatar.

---

## Phase B — Listen Together (core)

### Server model (in the proxy — these clients talk to the proxy, not the bot)
A **listen Session is a virtual transport** (no server-side audio):
`Session { id, code, host, members: {username → last_seen_ms},
track: TrackMeta?, queue: [TrackMeta], anchor_pos_ms, anchor_ts_ms (server epoch
ms), is_playing, rev }`, held in `AppState`:
`Arc<Mutex<HashMap<SessionId, Arc<Mutex<Session>>>>>` + a per-session `Notify`
(reuse the long-poll pattern). Ephemeral — pruned when empty; lost on restart
(rejoin).

Position is **derived**, never ticked by the server:
`live_pos = is_playing ? anchor_pos_ms + (now_ms − anchor_ts_ms) : anchor_pos_ms`.
`play/pause/seek/next` just **re-anchor** (`anchor_pos`, `anchor_ts`, `track`).

### Endpoints (mirror the remote relay)
- `POST /wave/api/listen/create` → `{ id, code }` (caller = host + first member).
- `POST /wave/api/listen/join` `{ code }` → `{ id, state }`.
- `POST /wave/api/listen/leave` `{ id }`.
- `GET  /wave/api/listen/state?id=&since=<rev>&wait=` → long-poll the session
  (track, queue, `anchorPosMs`, `anchorTsMs`, `isPlaying`, members[with profiles],
  rev). Also refreshes the caller's `last_seen` (presence).
- `POST /wave/api/listen/command?id=` `{ action, payload }` —
  `play{tracks,startIndex} | pause | resume | next | prev | seek{positionMs} |
  wave`. **Any member** (collaborative). Re-anchors + notifies.
- `GET  /wave/api/time` → `{ serverMs }` for clock-offset estimation.
- **Sweeper task (~1 s):** per session, auto-advance the virtual timeline past
  finished tracks (using each track's duration), prune empty/stale sessions,
  notify on change.

### Client — synced output (a third player mode: `local | discord | listen`)
- **Clock offset:** hit `GET /wave/api/time` ~5×, keep the sample with the
  smallest round-trip, `offset = serverMs + rtt/2 − localMs`; refresh
  occasionally. `serverNow = Date.now() + offset`.
- **Long-poll** listen state (`wait` + `since=rev`, AbortController) — same shape
  as remote mode.
- On each state, compute the **target** = `track` + `livePos` (anchor + serverNow):
  - **Track changed** → load the stream URL, seek to target, match play/pause.
  - **Same track** → **drift-correct**: if `|currentTime − target| > 0.75 s` →
    hard seek; else nudge `playbackRate` within `[0.97, 1.03]` proportional to
    drift, snapping back to `1.0` within ~50 ms tolerance (smooth, no audible
    jumps).
- **Controls** dispatch listen commands (collaborative), with optimistic UI.
- Each client streams the track **from songarr with its own account** — no audio
  relay, just like the bot.
- **UI:** a "Listen Together" control (start / join-by-code) near the Discord
  toggle; a room bar with **member avatars** + who's controlling + a
  copy-link/code button. `discord` and `listen` modes are mutually exclusive.

### Honest expectations
- Sync is **~100–300 ms** with continuous drift correction — tight enough to feel
  shared, **not** sample-accurate (that needs OS-level hooks). Per-device buffering
  is the thing we're fighting; the rate-nudge handles it.

### Heavy reuse
- Relay long-poll + `Notify`, room/claim/control concepts, and the position
  interpolation already written for the remote playbar (now it **drives local
  audio** instead of just displaying).

---

## Phase C — polish (later)
- Reactions/emoji + lightweight chat in the room.
- Invite **online friends** once a real friends list exists (replaces/augments
  the room code).
- **Bridge:** Discord voice listeners + web listeners on one shared timeline.
- Presence niceties (per-user "buffering"/"▶" indicators).

## Verification
- **A:** unit (profile fallback to username); build; curl set/get profile +
  avatar; UI shows name/avatar in sidebar + friends.
- **B:** unit the clock-offset + drift math; **two browser tabs** on one machine
  join a room and stay in sync through play/pause/seek/next; long-poll snappiness;
  sweeper auto-advance. Real cross-device drift needs multi-device testing on the
  deploy.
