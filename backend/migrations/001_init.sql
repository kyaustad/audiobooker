CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('root', 'user')),
    must_change_password INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    qbittorrent_url TEXT NOT NULL DEFAULT '',
    qbittorrent_username TEXT NOT NULL DEFAULT 'admin',
    qbittorrent_password TEXT NOT NULL DEFAULT '',
    download_path TEXT NOT NULL DEFAULT '/downloads',
    library_path TEXT NOT NULL DEFAULT '/audiobooks',
    path_template TEXT NOT NULL DEFAULT '{Author}/{Series}/{Title}',
    audible_region TEXT NOT NULL DEFAULT 'us',
    metadata_provider_url TEXT NOT NULL DEFAULT 'https://api.audnex.us',
    sync_interval_ms INTEGER NOT NULL DEFAULT 10000,
    vapid_public_key TEXT NOT NULL DEFAULT '',
    vapid_private_key TEXT NOT NULL DEFAULT '',
    vapid_subject TEXT NOT NULL DEFAULT 'mailto:admin@localhost',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO settings (id) VALUES (1);

CREATE TABLE IF NOT EXISTS api_keys (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    key_hash TEXT NOT NULL DEFAULT '',
    key_prefix TEXT NOT NULL DEFAULT '',
    created_at TEXT,
    rotated_at TEXT
);

INSERT OR IGNORE INTO api_keys (id) VALUES (1);

CREATE TABLE IF NOT EXISTS downloads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    magnet_uri TEXT NOT NULL,
    info_hash TEXT NOT NULL,
    name TEXT,
    status TEXT NOT NULL DEFAULT 'awaiting_match',
    progress REAL NOT NULL DEFAULT 0,
    download_speed INTEGER NOT NULL DEFAULT 0,
    eta INTEGER NOT NULL DEFAULT 0,
    save_path TEXT,
    content_path TEXT,
    destination_path TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    imported_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_downloads_user ON downloads(user_id);
CREATE INDEX IF NOT EXISTS idx_downloads_hash ON downloads(info_hash);
CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);

CREATE TABLE IF NOT EXISTS book_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    download_id INTEGER NOT NULL UNIQUE REFERENCES downloads(id) ON DELETE CASCADE,
    asin TEXT NOT NULL,
    title TEXT NOT NULL,
    subtitle TEXT,
    authors TEXT NOT NULL DEFAULT '[]',
    narrators TEXT NOT NULL DEFAULT '[]',
    series TEXT,
    series_index TEXT,
    cover_url TEXT,
    description TEXT,
    region TEXT NOT NULL DEFAULT 'us',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS push_subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL UNIQUE,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
