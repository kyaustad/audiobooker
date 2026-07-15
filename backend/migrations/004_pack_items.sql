-- Pack / collection support: per-torrent kind + mapped source items.

ALTER TABLE downloads ADD COLUMN kind TEXT NOT NULL DEFAULT 'single';

CREATE TABLE IF NOT EXISTS download_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    library_id INTEGER NOT NULL REFERENCES libraries(id),
    status TEXT NOT NULL DEFAULT 'pending',
    destination_path TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    imported_at TEXT,
    UNIQUE(download_id, source_path)
);

CREATE INDEX IF NOT EXISTS idx_download_items_download ON download_items(download_id);

-- Recreate book_metadata without UNIQUE(download_id); add download_item_id for pack rows.
CREATE TABLE book_metadata_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    download_item_id INTEGER UNIQUE REFERENCES download_items(id) ON DELETE CASCADE,
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

INSERT INTO book_metadata_new (
    id, download_id, download_item_id, asin, title, subtitle, authors, narrators,
    series, series_index, cover_url, description, region, created_at
)
SELECT
    id, download_id, NULL, asin, title, subtitle, authors, narrators,
    series, series_index, cover_url, description, region, created_at
FROM book_metadata;

DROP TABLE book_metadata;
ALTER TABLE book_metadata_new RENAME TO book_metadata;

CREATE INDEX IF NOT EXISTS idx_book_metadata_download ON book_metadata(download_id);
