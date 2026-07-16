-- Per-user web push event preferences.
-- Defaults avoid double-notifying for singles (download finished → import seconds later).
ALTER TABLE users ADD COLUMN notify_imported INTEGER NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN notify_download_finished INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN notify_pack_ready INTEGER NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN notify_failures INTEGER NOT NULL DEFAULT 1;
