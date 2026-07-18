use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Root,
    User,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Root => "root",
            Role::User => "user",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "root" => Some(Role::Root),
            "user" => Some(Role::User),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub must_change_password: bool,
    pub notify_imported: bool,
    pub notify_download_finished: bool,
    pub notify_pack_ready: bool,
    pub notify_failures: bool,
    pub abs_user_id: Option<String>,
    /// NULL = inherit global settings.
    pub rate_limit_requests: Option<i64>,
    pub rate_limit_window_secs: Option<i64>,
    pub rate_limit_active_torrents: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Column list for `User` FromRow queries (keep in sync with struct fields).
pub const USER_COLUMNS: &str = "id, username, password_hash, role, must_change_password, \
    notify_imported, notify_download_finished, notify_pack_ready, notify_failures, \
    abs_user_id, rate_limit_requests, rate_limit_window_secs, rate_limit_active_torrents, \
    created_at, updated_at";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPrefs {
    pub notify_imported: bool,
    pub notify_download_finished: bool,
    pub notify_pack_ready: bool,
    pub notify_failures: bool,
}

impl From<&User> for NotificationPrefs {
    fn from(u: &User) -> Self {
        Self {
            notify_imported: u.notify_imported,
            notify_download_finished: u.notify_download_finished,
            notify_pack_ready: u.notify_pack_ready,
            notify_failures: u.notify_failures,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    Imported,
    DownloadFinished,
    PackReady,
    Failure,
}

impl NotificationPrefs {
    pub fn allows(&self, kind: NotifyKind) -> bool {
        match kind {
            NotifyKind::Imported => self.notify_imported,
            NotifyKind::DownloadFinished => self.notify_download_finished,
            NotifyKind::PackReady => self.notify_pack_ready,
            NotifyKind::Failure => self.notify_failures,
        }
    }
}

impl User {
    pub fn is_root(&self) -> bool {
        self.role == "root"
    }

    pub fn notification_prefs(&self) -> NotificationPrefs {
        NotificationPrefs::from(self)
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Settings {
    pub id: i64,
    pub qbittorrent_url: String,
    pub qbittorrent_username: String,
    #[serde(skip_serializing)]
    pub qbittorrent_password: String,
    pub download_path: String,
    pub library_path: String,
    pub path_template: String,
    pub audible_region: String,
    pub metadata_provider_url: String,
    pub sync_interval_ms: i64,
    pub vapid_public_key: String,
    #[serde(skip_serializing)]
    pub vapid_private_key: String,
    pub vapid_subject: String,
    pub audiobookshelf_url: String,
    #[serde(skip_serializing)]
    pub audiobookshelf_token: String,
    pub abs_user_sync_enabled: bool,
    pub abs_user_sync_interval_ms: i64,
    #[serde(skip_serializing)]
    pub abs_user_default_password: String,
    pub abs_user_sync_libraries: bool,
    pub abs_user_last_sync_at: Option<String>,
    /// 0 = unlimited.
    pub rate_limit_requests: i64,
    pub rate_limit_window_secs: i64,
    pub rate_limit_active_torrents: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsPublic {
    pub qbittorrent_url: String,
    pub qbittorrent_username: String,
    pub qbittorrent_password_set: bool,
    pub download_path: String,
    pub library_path: String,
    pub path_template: String,
    pub audible_region: String,
    pub metadata_provider_url: String,
    pub sync_interval_ms: i64,
    pub vapid_public_key: String,
    pub vapid_configured: bool,
    pub audiobookshelf_url: String,
    pub audiobookshelf_token_set: bool,
    pub abs_user_sync_enabled: bool,
    pub abs_user_sync_interval_ms: i64,
    pub abs_user_default_password_set: bool,
    pub abs_user_sync_libraries: bool,
    pub abs_user_last_sync_at: Option<String>,
    pub rate_limit_requests: i64,
    pub rate_limit_window_secs: i64,
    pub rate_limit_active_torrents: i64,
}

impl From<Settings> for SettingsPublic {
    fn from(s: Settings) -> Self {
        Self {
            qbittorrent_url: s.qbittorrent_url,
            qbittorrent_username: s.qbittorrent_username,
            qbittorrent_password_set: !s.qbittorrent_password.is_empty(),
            download_path: s.download_path,
            library_path: s.library_path,
            path_template: s.path_template,
            audible_region: s.audible_region,
            metadata_provider_url: s.metadata_provider_url,
            sync_interval_ms: s.sync_interval_ms,
            vapid_public_key: s.vapid_public_key.clone(),
            vapid_configured: !s.vapid_public_key.is_empty() && !s.vapid_private_key.is_empty(),
            audiobookshelf_url: s.audiobookshelf_url,
            audiobookshelf_token_set: !s.audiobookshelf_token.is_empty(),
            abs_user_sync_enabled: s.abs_user_sync_enabled,
            abs_user_sync_interval_ms: s.abs_user_sync_interval_ms,
            abs_user_default_password_set: !s.abs_user_default_password.is_empty(),
            abs_user_sync_libraries: s.abs_user_sync_libraries,
            abs_user_last_sync_at: s.abs_user_last_sync_at,
            rate_limit_requests: s.rate_limit_requests,
            rate_limit_window_secs: s.rate_limit_window_secs,
            rate_limit_active_torrents: s.rate_limit_active_torrents,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Library {
    pub id: i64,
    pub name: String,
    /// Writable root path inside the Audiobooker container.
    pub path: String,
    pub abs_id: Option<String>,
    /// Path reported by Audiobookshelf (informational; may not exist here).
    pub abs_path: Option<String>,
    pub created_at: String,
}

impl Library {
    /// Placeholder until an admin sets the container mount path.
    pub fn path_needs_config(path: &str) -> bool {
        let p = path.trim();
        p.is_empty() || p.starts_with("__unset__")
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Download {
    pub id: i64,
    pub user_id: i64,
    pub magnet_uri: String,
    pub info_hash: String,
    pub name: Option<String>,
    pub status: String,
    pub progress: f64,
    pub download_speed: i64,
    pub eta: i64,
    pub save_path: Option<String>,
    pub content_path: Option<String>,
    pub destination_path: Option<String>,
    pub error_message: Option<String>,
    pub library_id: Option<i64>,
    pub kind: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub imported_at: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DownloadItem {
    pub id: i64,
    pub download_id: i64,
    pub source_path: String,
    pub library_id: i64,
    pub status: String,
    pub destination_path: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub imported_at: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct BookMetadata {
    pub id: i64,
    pub download_id: i64,
    pub download_item_id: Option<i64>,
    pub asin: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: String,
    pub narrators: String,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub region: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct DownloadItemSource {
    pub id: i64,
    pub download_id: i64,
    pub download_item_id: i64,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadItemWithMetadata {
    #[serde(flatten)]
    pub item: DownloadItem,
    pub source_paths: Vec<String>,
    pub metadata: Option<BookMetadataPublic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadWithMetadata {
    #[serde(flatten)]
    pub download: Download,
    pub metadata: Option<BookMetadataPublic>,
    #[serde(default)]
    pub items: Vec<DownloadItemWithMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BookMetadataPublic {
    pub asin: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub narrators: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub region: String,
}

impl From<BookMetadata> for BookMetadataPublic {
    fn from(m: BookMetadata) -> Self {
        Self {
            asin: m.asin,
            title: m.title,
            subtitle: m.subtitle,
            authors: serde_json::from_str(&m.authors).unwrap_or_default(),
            narrators: serde_json::from_str(&m.narrators).unwrap_or_default(),
            series: m.series,
            series_index: m.series_index,
            cover_url: m.cover_url,
            description: m.description,
            region: m.region,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub must_change_password: bool,
}

impl From<User> for AuthUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            role: u.role,
            must_change_password: u.must_change_password,
        }
    }
}

#[allow(dead_code)]
pub type Timestamp = DateTime<Utc>;
