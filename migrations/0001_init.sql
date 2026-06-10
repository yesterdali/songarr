CREATE TABLE virtual_tracks (
  id TEXT PRIMARY KEY,              -- sgr_…
  provider TEXT NOT NULL,           -- deezer | ytmusic
  provider_track_id TEXT NOT NULL,
  artist TEXT NOT NULL,
  title TEXT NOT NULL,
  album TEXT,
  duration_ms INTEGER,
  isrc TEXT,
  artwork_url TEXT,
  status TEXT NOT NULL DEFAULT 'virtual',
        -- virtual | streaming | staged | imported | failed
  real_subsonic_id TEXT,            -- filled after import
  fail_reason TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(provider, provider_track_id)
);

CREATE TABLE stream_jobs (
  id TEXT PRIMARY KEY,
  virtual_track_id TEXT NOT NULL REFERENCES virtual_tracks(id),
  requested_by TEXT NOT NULL,       -- subsonic username
  source_url TEXT,                  -- resolved YouTube URL
  match_score INTEGER,
  staging_path TEXT,
  status TEXT NOT NULL,             -- resolving | piping | finalizing | imported
                                    -- | needs_review | failed
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE pending_actions (      -- scrobbles/stars received for virtual ids
  id TEXT PRIMARY KEY,
  virtual_track_id TEXT NOT NULL,
  username TEXT NOT NULL,
  action TEXT NOT NULL,             -- scrobble | star
  payload_json TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE upgrade_requests (
  id TEXT PRIMARY KEY,
  virtual_track_id TEXT NOT NULL,
  status TEXT NOT NULL,             -- pending | sent | done | unsupported
  created_at TEXT NOT NULL
);

CREATE INDEX idx_stream_jobs_virtual_track ON stream_jobs(virtual_track_id);
CREATE INDEX idx_stream_jobs_status ON stream_jobs(status);
CREATE INDEX idx_pending_actions_virtual_track ON pending_actions(virtual_track_id);
CREATE INDEX idx_virtual_tracks_status ON virtual_tracks(status);
