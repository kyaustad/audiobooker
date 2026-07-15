-- Audiobookshelf libraries (container paths that match ABS library folders)
CREATE TABLE IF NOT EXISTS libraries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL UNIQUE,
    abs_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS user_libraries (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, library_id)
);

-- Applied carefully in Rust when missing:
-- ALTER TABLE downloads ADD COLUMN library_id INTEGER REFERENCES libraries(id);
-- ALTER TABLE settings ADD COLUMN audiobookshelf_url TEXT NOT NULL DEFAULT '';
-- ALTER TABLE settings ADD COLUMN audiobookshelf_token TEXT NOT NULL DEFAULT '';
