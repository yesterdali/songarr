-- Per-user personalization: a display name + avatar, decoupled from the
-- Navidrome login (which stays the auth identity). Shown in the sidebar, friend
-- activity, and listen rooms. Avatars are small (client resizes before upload).
CREATE TABLE user_profile (
  username TEXT PRIMARY KEY,
  display_name TEXT,
  avatar_blob BLOB,
  avatar_mime TEXT,
  updated_at_epoch INTEGER NOT NULL
);
