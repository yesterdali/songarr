-- Songarr Wave adaptive feedback. This is intentionally separate from
-- Navidrome stars: stars are portable user state, feedback is wave tuning.

CREATE TABLE wave_feedback (
  username TEXT NOT NULL,
  track_id TEXT,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  song_key TEXT NOT NULL,
  artist_key TEXT NOT NULL,
  action TEXT NOT NULL,          -- play | skip | like | dislike
  created_at_epoch INTEGER NOT NULL,
  PRIMARY KEY (username, song_key, action)
);

CREATE INDEX idx_wave_feedback_user_time
  ON wave_feedback(username, created_at_epoch);

CREATE INDEX idx_wave_feedback_user_action_time
  ON wave_feedback(username, action, created_at_epoch);
