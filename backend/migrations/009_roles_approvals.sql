-- Widen roles (requester | user | approver | root) and add remove permissions.
-- SQLite cannot alter CHECK constraints in place - rebuild users.

PRAGMA foreign_keys=OFF;

CREATE TABLE users_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('root', 'requester', 'user', 'approver')),
    must_change_password INTEGER NOT NULL DEFAULT 0,
    notify_imported INTEGER NOT NULL DEFAULT 1,
    notify_download_finished INTEGER NOT NULL DEFAULT 0,
    notify_pack_ready INTEGER NOT NULL DEFAULT 1,
    notify_failures INTEGER NOT NULL DEFAULT 1,
    abs_user_id TEXT,
    rate_limit_requests INTEGER,
    rate_limit_window_secs INTEGER,
    rate_limit_active_torrents INTEGER,
    can_remove INTEGER NOT NULL DEFAULT 1,
    can_remove_files INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO users_new (
    id, username, password_hash, role, must_change_password,
    notify_imported, notify_download_finished, notify_pack_ready, notify_failures,
    abs_user_id, rate_limit_requests, rate_limit_window_secs, rate_limit_active_torrents,
    can_remove, can_remove_files, created_at, updated_at
)
SELECT
    id, username, password_hash, role, must_change_password,
    COALESCE(notify_imported, 1),
    COALESCE(notify_download_finished, 0),
    COALESCE(notify_pack_ready, 1),
    COALESCE(notify_failures, 1),
    abs_user_id, rate_limit_requests, rate_limit_window_secs, rate_limit_active_torrents,
    1, 0, created_at, updated_at
FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;

PRAGMA foreign_keys=ON;

-- Singles can opt into pack-style file mapping after download.
ALTER TABLE downloads ADD COLUMN map_files INTEGER NOT NULL DEFAULT 0;
