use sqlx::SqlitePool;

use crate::abb::AbbClient;
use crate::config::Config;
use crate::metadata::MetadataClient;
use crate::qbittorrent::QbittorrentClient;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
    pub qb: QbittorrentClient,
    pub metadata: MetadataClient,
    pub abb: AbbClient,
}
