-- Cache AudiobookBay listings/pages/details discovered via Discover to cut repeat ABB hits.

CREATE TABLE IF NOT EXISTS abb_page_cache (
    cache_key TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    query_key TEXT NOT NULL DEFAULT '',
    page INTEGER NOT NULL,
    has_more INTEGER NOT NULL DEFAULT 0,
    results_json TEXT NOT NULL,
    mirror TEXT NOT NULL DEFAULT '',
    category_label TEXT,
    fetched_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_abb_page_cache_expires ON abb_page_cache(expires_at);

CREATE TABLE IF NOT EXISTS abb_listings (
    url TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    cover_url TEXT,
    info TEXT,
    author TEXT,
    language TEXT,
    format TEXT,
    bitrate TEXT,
    size TEXT,
    posted TEXT,
    category TEXT,
    discovered_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_abb_listings_title ON abb_listings(title);
CREATE INDEX IF NOT EXISTS idx_abb_listings_author ON abb_listings(author);
CREATE INDEX IF NOT EXISTS idx_abb_listings_updated ON abb_listings(updated_at);

CREATE TABLE IF NOT EXISTS abb_details (
    url TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    info_hash TEXT,
    magnet_uri TEXT,
    cover_url TEXT,
    description TEXT,
    author TEXT,
    narrator TEXT,
    format TEXT,
    bitrate TEXT,
    size TEXT,
    fetched_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_abb_details_expires ON abb_details(expires_at);
