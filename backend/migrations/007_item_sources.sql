-- Multi-file pack items: many torrent-relative paths → one Audible book.
CREATE TABLE IF NOT EXISTS download_item_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    download_item_id INTEGER NOT NULL REFERENCES download_items(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    UNIQUE(download_item_id, source_path),
    UNIQUE(download_id, source_path)
);

CREATE INDEX IF NOT EXISTS idx_download_item_sources_item ON download_item_sources(download_item_id);
CREATE INDEX IF NOT EXISTS idx_download_item_sources_download ON download_item_sources(download_id);

-- Backfill from existing single-path items.
INSERT OR IGNORE INTO download_item_sources (download_id, download_item_id, source_path)
SELECT download_id, id, source_path FROM download_items;
