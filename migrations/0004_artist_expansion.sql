-- Artist expansion: external albums shown under a real Navidrome artist.
-- Tracks are embedded in payload_json for v1 because callers fetch one album
-- and list its tracks; no cross-album track query exists yet.

CREATE TABLE virtual_albums (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_album_id TEXT NOT NULL,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  album_type TEXT,
  release_date TEXT,
  artwork_url TEXT,
  track_count INTEGER,
  payload_json TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'virtual',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(provider, provider_album_id)
);

CREATE INDEX idx_virtual_albums_artist ON virtual_albums(artist);

CREATE TABLE artist_catalog_cache (
  provider TEXT NOT NULL,
  artist_key TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  fetched_at_epoch INTEGER NOT NULL,
  PRIMARY KEY(provider, artist_key)
);
