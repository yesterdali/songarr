-- Lyrics fetched from external providers, keyed by song identity (not by
-- audio source) so Navidrome files, YouTube-resolved virtual tracks, and
-- future providers (VK) all share one row. provider='none' is a negative
-- cache entry: the song was looked up and had no lyrics; retried after TTL.

CREATE TABLE lyrics_cache (
  song_key TEXT NOT NULL PRIMARY KEY,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  duration_secs INTEGER,           -- duration the synced timestamps align to
  provider TEXT NOT NULL,          -- lrclib | none
  plain TEXT,                      -- plain-text lyrics, NULL if unavailable
  synced_json TEXT,                -- JSON [{"start_ms":int,"value":str}], NULL if unavailable
  fetched_at_epoch INTEGER NOT NULL
);
