-- Cached yt-dlp resolution per virtual track, filled speculatively at
-- search time so that pressing play skips the search round-trip.
ALTER TABLE virtual_tracks ADD COLUMN resolved_url TEXT;
ALTER TABLE virtual_tracks ADD COLUMN resolved_score INTEGER;
ALTER TABLE virtual_tracks ADD COLUMN resolved_title TEXT;
ALTER TABLE virtual_tracks ADD COLUMN resolved_at_epoch INTEGER;
