-- Spotify-Connect-style remote control: the web/desktop app drives the Discord
-- bot through Songarr as a relay. Commands flow app→bot; state flows bot→app.

-- Append-only per-user command log. The bot consumes in seq order, tracking the
-- last seq it ran, and prunes consumed rows.
CREATE TABLE remote_command (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  username TEXT NOT NULL,
  action TEXT NOT NULL,          -- connect | disconnect | play | pause | resume | next | prev | seek
  payload TEXT,                  -- JSON
  created_at_epoch INTEGER NOT NULL
);
CREATE INDEX idx_remote_command_user_seq ON remote_command(username, seq);

-- The bot's reported playback state (one row per user). updated_at doubles as a
-- liveness heartbeat for the "connected" indicator.
CREATE TABLE remote_state (
  username TEXT PRIMARY KEY,
  connected INTEGER NOT NULL DEFAULT 0,
  track_id TEXT,
  title TEXT,
  artist TEXT,
  album TEXT,
  cover_art TEXT,
  position_ms INTEGER,
  duration_ms INTEGER,
  is_playing INTEGER NOT NULL DEFAULT 0,
  queue_json TEXT,
  updated_at_epoch INTEGER NOT NULL
);
