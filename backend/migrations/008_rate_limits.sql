-- Global download rate limits (0 = unlimited).
ALTER TABLE settings ADD COLUMN rate_limit_requests INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN rate_limit_window_secs INTEGER NOT NULL DEFAULT 86400;
ALTER TABLE settings ADD COLUMN rate_limit_active_torrents INTEGER NOT NULL DEFAULT 0;

-- Per-user overrides (NULL = inherit global).
ALTER TABLE users ADD COLUMN rate_limit_requests INTEGER;
ALTER TABLE users ADD COLUMN rate_limit_window_secs INTEGER;
ALTER TABLE users ADD COLUMN rate_limit_active_torrents INTEGER;
