-- Recommendations (songarr-recs-plan.md). Tables for R1–R3: listens is the
-- personalization fuel (R3), rec_cache the per-source response cache (R2),
-- rec_shown the anti-repetition log (R2). Created together so the schema is
-- stable while the features land incrementally.

-- every play the proxy observes, real or virtual
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
  payload_json TEXT NOT NULL,
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
