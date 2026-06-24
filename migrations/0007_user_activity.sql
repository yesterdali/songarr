-- Friend Activity: each user's most-recent "now playing" track, surfaced as a
-- Spotify-style friends feed. One row per user (latest play wins). For now
-- every account on the instance is treated as a friend.

CREATE TABLE user_activity (
  username TEXT PRIMARY KEY,
  song_id TEXT NOT NULL,
  title TEXT NOT NULL,
  artist TEXT NOT NULL,
  album TEXT,
  cover_art TEXT,
  updated_at_epoch INTEGER NOT NULL
);

CREATE INDEX idx_user_activity_time ON user_activity(updated_at_epoch);
