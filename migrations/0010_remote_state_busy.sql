-- "busy" flag for remote_state: the bot is in use in another voice channel of
-- this server, so this user can't take it over (shared-room exclusivity).
ALTER TABLE remote_state ADD COLUMN busy INTEGER NOT NULL DEFAULT 0;
