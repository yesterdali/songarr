-- Monotonic revision for remote_state so the app's long-poll can ask "give me
-- state newer than rev N" without the 1-second epoch granularity missing fast
-- back-to-back updates (e.g. a skip).
ALTER TABLE remote_state ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
