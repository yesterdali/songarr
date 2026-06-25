# Multi-user shared Discord remote ("rooms")

## Context
The remote bot currently keys playback **per linked user**, so two people on the
same server clobber each other's queue — and a bot can only hold **one voice
channel per server** anyway. Move to a shared-room model:

- The bot in a voice channel is **one shared queue**; everyone in that channel
  controls it together.
- **Exclusive per server:** while the bot is busy in a channel, anyone in a
  *different* channel of that server gets "busy" instead of stealing it.
- **Auto-release:** the bot leaves voice after ~2 min idle (paused/stopped/empty
  queue) or when the channel has no human listeners.

Multiple accounts per server already works (each runs `/link`). Cross-server stays
independent — one room per guild, the bot can be in one channel per guild.

## Model
- **Room** keyed by `GuildId`: `{ channel_id, metas(uuid→meta), tracks, wave,
  not_playing_since }`. `Rooms = Arc<Mutex<HashMap<GuildId, Arc<Mutex<Room>>>>>`
  (outer lock held only briefly to find/insert; per-room lock serializes that
  room's playback ops).
- **Control = your linked Discord account is currently in the room's voice
  channel.** Commands from outside the active room are ignored (except connect).
- **Connect resolves to:** `Controlling` (claimed or joined) · `Busy` (bot is in
  another channel of your server) · `NotInVoice`.

## Bot (`remote.rs` rewrite)
- `run`: create the shared `Rooms`; spawn a **watchdog** + one **long-poll task
  per linked user** (per-user command intake + per-user state output stay; the
  playback state becomes shared per guild).
- `user_loop`: long-poll commands; `connect` → claim/join or report busy;
  `play/pause/next/prev/seek/wave` apply to the room **only if you're in it**;
  report your state each tick while active.
- **Claim is atomic** via the outer map lock: insert a placeholder room *before*
  the songbird join, roll back on join failure, so two simultaneous claimers
  can't double-join.
- **Watchdog (~15s):** per room — leave + drop the room if the bot isn't actually
  in voice, if the channel has no human members, or if it's been not-playing for
  longer than the idle timeout (`REMOTE_IDLE_SECS = 120`).
- **State per user:** `Controlling` → the room's playback (track/pos/queue);
  `Busy` → `connected:false` + `busy:true`; else `connected:false`.

## Proxy
- Migration `0010`: add `busy` to `remote_state`.
- Thread `busy` through `RemoteStateReport` / `RemoteStateRow` / `RemoteStateResponse`
  (+ the upsert & select). The relay/long-poll is otherwise unchanged.

## Web
- `RemoteState.busy`; parse it in `getRemoteState`.
- `DiscordConnectToggle`: show **"Vivaldi занят"** (amber) when `busy`; clicking
  again gives up (disconnect). The playbar/controls already bind to the shared
  state, so multiple people in one channel see the same playback.

## Decisions (baked in)
- Exclusivity scoped **per server** (different servers run independent rooms).
- Idle timeout **120 s**; also leave when the channel is empty of humans.
- Disconnecting your app stops *your* control/state but does **not** stop the
  shared music — the watchdog frees the bot on idle/empty.

## Verification
- `cargo build` (proxy + bot), web typecheck/build + tests, migrations run.
- Long-poll smoke still green; room transitions reasoned through (live voice with
  multiple users needs the deployed bot to exercise fully).
