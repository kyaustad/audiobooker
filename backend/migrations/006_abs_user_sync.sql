-- Link Audiobooker users to ABS users + root-configurable sync.
ALTER TABLE users ADD COLUMN abs_user_id TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_abs_user_id ON users(abs_user_id) WHERE abs_user_id IS NOT NULL;

ALTER TABLE settings ADD COLUMN abs_user_sync_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN abs_user_sync_interval_ms INTEGER NOT NULL DEFAULT 3600000;
ALTER TABLE settings ADD COLUMN abs_user_default_password TEXT NOT NULL DEFAULT 'changeme';
ALTER TABLE settings ADD COLUMN abs_user_sync_libraries INTEGER NOT NULL DEFAULT 1;
ALTER TABLE settings ADD COLUMN abs_user_last_sync_at TEXT;
