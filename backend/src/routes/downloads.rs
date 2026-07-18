use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::files::{entries_from_disk, entries_from_qb_paths, remove_library_destination, resolve_download_source};
use crate::limits::{check_active_torrent_limit, check_request_limit};
use crate::magnet::parse_download_input;
use crate::metadata::MetadataMatch;
use crate::models::{
    BookMetadata, Download, DownloadItem, DownloadItemSource, DownloadItemWithMetadata,
    DownloadWithMetadata, Library, Settings, USER_COLUMNS, User,
};
use crate::qbittorrent::{QbTorrent, map_state};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateDownloadRequest {
    pub input: String,
    pub name: Option<String>,
    pub kind: Option<String>,
}

#[derive(Deserialize)]
pub struct MatchRequest {
    pub match_data: MetadataMatch,
    pub library_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct MapItemRequest {
    /// Legacy single path; prefer `source_paths`.
    pub source_path: Option<String>,
    pub source_paths: Option<Vec<String>>,
    pub match_data: MetadataMatch,
    pub library_id: Option<i64>,
}

fn normalize_source_path(raw: &str) -> AppResult<String> {
    let source_path = raw.trim().trim_start_matches('/').replace('\\', "/");
    if source_path.is_empty() || source_path.contains("..") {
        return Err(AppError::BadRequest("Invalid source path".into()));
    }
    Ok(source_path)
}

fn collect_map_paths(body: &MapItemRequest) -> AppResult<Vec<String>> {
    let mut paths = Vec::new();
    if let Some(list) = &body.source_paths {
        for p in list {
            let n = normalize_source_path(p)?;
            if !paths.contains(&n) {
                paths.push(n);
            }
        }
    }
    if let Some(single) = &body.source_path {
        let n = normalize_source_path(single)?;
        if !paths.contains(&n) {
            paths.push(n);
        }
    }
    if paths.is_empty() {
        return Err(AppError::BadRequest(
            "Select at least one file or folder to map".into(),
        ));
    }
    Ok(paths)
}

async fn source_paths_for_item(
    pool: &sqlx::SqlitePool,
    item: &DownloadItem,
) -> AppResult<Vec<String>> {
    let rows = sqlx::query_as::<_, DownloadItemSource>(
        "SELECT * FROM download_item_sources WHERE download_item_id = ? ORDER BY source_path",
    )
    .bind(item.id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(vec![item.source_path.clone()]);
    }
    Ok(rows.into_iter().map(|r| r.source_path).collect())
}

async fn attach_metadata(
    pool: &sqlx::SqlitePool,
    download: Download,
) -> AppResult<DownloadWithMetadata> {
    let meta = sqlx::query_as::<_, BookMetadata>(
        "SELECT * FROM book_metadata WHERE download_id = ? AND download_item_id IS NULL",
    )
    .bind(download.id)
    .fetch_optional(pool)
    .await?;

    let items = if download.kind == "pack" {
        let rows = sqlx::query_as::<_, DownloadItem>(
            "SELECT * FROM download_items WHERE download_id = ? ORDER BY source_path",
        )
        .bind(download.id)
        .fetch_all(pool)
        .await?;
        let mut out = Vec::new();
        for item in rows {
            let source_paths = source_paths_for_item(pool, &item).await?;
            let item_meta = sqlx::query_as::<_, BookMetadata>(
                "SELECT * FROM book_metadata WHERE download_item_id = ?",
            )
            .bind(item.id)
            .fetch_optional(pool)
            .await?;
            out.push(DownloadItemWithMetadata {
                item,
                source_paths,
                metadata: item_meta.map(Into::into),
            });
        }
        out
    } else {
        Vec::new()
    };

    Ok(DownloadWithMetadata {
        download,
        metadata: meta.map(Into::into),
        items,
    })
}

async fn load_user_download(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    id: i64,
) -> AppResult<Download> {
    sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

async fn resolve_library_id(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    requested: Option<i64>,
) -> AppResult<i64> {
    let allowed = sqlx::query_as::<_, (i64, String)>(
        r#"
        SELECT l.id, l.path FROM libraries l
        INNER JOIN user_libraries ul ON ul.library_id = l.id
        WHERE ul.user_id = ?
        ORDER BY l.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    if allowed.is_empty() {
        return Err(AppError::BadRequest(
            "Your account has no libraries assigned. Ask an admin to grant library access.".into(),
        ));
    }
    let library_id = if let Some(id) = requested {
        if !allowed.iter().any(|(lid, _)| *lid == id) {
            return Err(AppError::Forbidden);
        }
        id
    } else if allowed.len() == 1 {
        allowed[0].0
    } else {
        return Err(AppError::BadRequest(
            "Select which Audiobookshelf library to import into".into(),
        ));
    };
    if let Some((_, path)) = allowed.iter().find(|(lid, _)| *lid == library_id) {
        if Library::path_needs_config(path) {
            return Err(AppError::BadRequest(
                "That library has no container path set. Ask an admin to set it under Settings → Libraries."
                    .into(),
            ));
        }
    }
    Ok(library_id)
}

pub async fn list(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(auth.user.id)
    .fetch_all(&state.pool)
    .await?;

    let mut downloads = Vec::new();
    for row in rows {
        downloads.push(attach_metadata(&state.pool, row).await?);
    }
    Ok(Json(json!({ "downloads": downloads })))
}

pub async fn list_all_for_api(
    State(state): State<AppState>,
    auth: AuthSession,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let rows = sqlx::query_as::<_, Download>("SELECT * FROM downloads ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await?;
    let mut downloads = Vec::new();
    for row in rows {
        downloads.push(attach_metadata(&state.pool, row).await?);
    }
    Ok(Json(json!({ "downloads": downloads })))
}

pub async fn list_for_username(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(username): AxumPath<String>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let user = sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE username = ? COLLATE NOCASE"
    ))
    .bind(username)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let rows = sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let mut downloads = Vec::new();
    for row in rows {
        downloads.push(attach_metadata(&state.pool, row).await?);
    }
    Ok(Json(json!({ "username": user.username, "downloads": downloads })))
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<CreateDownloadRequest>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::BadRequest(
            "Root account is for administration only. Create a user account to add downloads.".into(),
        ));
    }

    check_request_limit(&state.pool, auth.user.id).await?;

    let kind = match body.kind.as_deref().unwrap_or("single") {
        "pack" => "pack",
        "single" | "" => "single",
        other => {
            return Err(AppError::BadRequest(format!(
                "Invalid kind '{other}' (use single or pack)"
            )));
        }
    };

    let parsed = parse_download_input(&body.input, body.name.as_deref())
        .ok_or_else(|| AppError::BadRequest("Invalid magnet link or info hash".into()))?;

    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM downloads WHERE user_id = ? AND info_hash = ? AND status != 'error' LIMIT 1",
    )
    .bind(auth.user.id)
    .bind(&parsed.info_hash)
    .fetch_optional(&state.pool)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict(
            "This torrent is already in your downloads".into(),
        ));
    }

    let result = sqlx::query(
        r#"
        INSERT INTO downloads (user_id, magnet_uri, info_hash, name, status, kind)
        VALUES (?, ?, ?, ?, 'awaiting_match', ?)
        "#,
    )
    .bind(auth.user.id)
    .bind(&parsed.magnet_uri)
    .bind(&parsed.info_hash)
    .bind(&parsed.name)
    .bind(kind)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(result.last_insert_rowid())
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

/// Start a pack torrent in qBittorrent without Audible matching.
pub async fn start_pack(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.status != "awaiting_match" {
        return Err(AppError::BadRequest(
            "Pack download already started".into(),
        ));
    }

    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    if settings.qbittorrent_url.is_empty() {
        return Err(AppError::BadRequest(
            "qBittorrent is not configured. Ask an administrator to finish setup.".into(),
        ));
    }

    // Ensure the user has at least one library for later mapping.
    let lib_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_libraries WHERE user_id = ?",
    )
    .bind(auth.user.id)
    .fetch_one(&state.pool)
    .await?;
    if lib_count.0 == 0 {
        return Err(AppError::BadRequest(
            "Your account has no libraries assigned. Ask an admin to grant library access.".into(),
        ));
    }

    check_active_torrent_limit(&state.pool, auth.user.id).await?;

    sqlx::query(
        "UPDATE downloads SET kind = 'pack', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    state
        .qb
        .add_magnet(
            &settings.qbittorrent_url,
            &settings.qbittorrent_username,
            &settings.qbittorrent_password,
            &download.magnet_uri,
            Some("audiobooks"),
        )
        .await?;

    sqlx::query(
        "UPDATE downloads SET status = 'queued', kind = 'pack', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

pub async fn list_files(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let mut download = load_user_download(&state.pool, auth.user.id, id).await?;
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    // Refresh save/content paths from qBit so incomplete→complete moves are visible.
    if !settings.qbittorrent_url.is_empty() {
        if let Ok(torrents) = state
            .qb
            .list_torrents(
                &settings.qbittorrent_url,
                &settings.qbittorrent_username,
                &settings.qbittorrent_password,
            )
            .await
        {
            if let Some(torrent) = torrents
                .iter()
                .find(|t| t.hash.eq_ignore_ascii_case(&download.info_hash))
            {
                sqlx::query(
                    "UPDATE downloads SET save_path = ?, content_path = ?, updated_at = datetime('now') WHERE id = ?",
                )
                .bind(&torrent.save_path)
                .bind(&torrent.content_path)
                .bind(download.id)
                .execute(&state.pool)
                .await?;
                download.save_path = Some(torrent.save_path.clone());
                download.content_path = Some(torrent.content_path.clone());
            }
        }
    }

    let mut entries = Vec::new();
    let mut source = "none";

    if !settings.qbittorrent_url.is_empty()
        && !matches!(download.status.as_str(), "awaiting_match")
    {
        if let Ok(files) = state
            .qb
            .torrent_files(
                &settings.qbittorrent_url,
                &settings.qbittorrent_username,
                &settings.qbittorrent_password,
                &download.info_hash,
            )
            .await
        {
            if !files.is_empty() {
                let pairs: Vec<_> = files
                    .into_iter()
                    .map(|f| (f.name, f.size))
                    .collect();
                entries = entries_from_qb_paths(&pairs);
                source = "qbittorrent";
            }
        }
    }

    if entries.is_empty() {
        let root = resolve_download_source(
            download.content_path.as_deref(),
            download.save_path.as_deref(),
            &settings.download_path,
        );
        if root.exists() {
            entries = entries_from_disk(&root).await?;
            source = "disk";
        }
    }

    Ok(Json(json!({
        "files": entries,
        "source": source,
        "content_path": download.content_path,
        "save_path": download.save_path,
    })))
}

pub async fn retry_pack_imports(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.kind != "pack" {
        return Err(AppError::BadRequest("Only pack downloads support retry".into()));
    }

    let updated = sqlx::query(
        r#"
        UPDATE download_items SET
            status = 'ready',
            error_message = NULL,
            updated_at = datetime('now')
        WHERE download_id = ? AND status IN ('error', 'copying')
        "#,
    )
    .bind(download.id)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::BadRequest(
            "No failed or stuck imports to retry".into(),
        ));
    }

    sqlx::query(
        "UPDATE downloads SET status = 'completed', error_message = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "retried": updated,
        "download": attach_metadata(&state.pool, download).await?
    })))
}

/// Re-queue a single-book import after a stuck `copying` or failed import.
pub async fn retry_import(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.kind == "pack" {
        return Err(AppError::BadRequest(
            "Use pack retry-imports for pack downloads".into(),
        ));
    }
    if !matches!(download.status.as_str(), "copying" | "error") {
        return Err(AppError::BadRequest(
            "Only stuck or failed imports can be retried".into(),
        ));
    }

    let meta: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM book_metadata WHERE download_id = ? AND download_item_id IS NULL",
    )
    .bind(download.id)
    .fetch_optional(&state.pool)
    .await?;
    if meta.is_none() {
        return Err(AppError::BadRequest(
            "Cannot retry import without Audible metadata".into(),
        ));
    }

    sqlx::query(
        r#"
        UPDATE downloads SET
            status = 'completed',
            error_message = NULL,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

/// Re-copy an already-imported single book into the library, overwriting the destination.
pub async fn reimport(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.kind == "pack" {
        return Err(AppError::BadRequest(
            "Pack books use Un-import / remap on the Map pack page".into(),
        ));
    }
    if download.status != "imported" {
        return Err(AppError::BadRequest(
            "Only imported singles can be re-imported".into(),
        ));
    }

    let meta: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM book_metadata WHERE download_id = ? AND download_item_id IS NULL",
    )
    .bind(download.id)
    .fetch_optional(&state.pool)
    .await?;
    if meta.is_none() {
        return Err(AppError::BadRequest(
            "Cannot re-import without Audible metadata".into(),
        ));
    }

    if let Some(dest) = download
        .destination_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let library_root = if let Some(lid) = download.library_id {
            let library = sqlx::query_as::<_, Library>(
                "SELECT id, name, path, abs_id, abs_path, created_at FROM libraries WHERE id = ?",
            )
            .bind(lid)
            .fetch_optional(&state.pool)
            .await?;
            library.map(|l| l.path)
        } else {
            let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
                .fetch_one(&state.pool)
                .await?;
            if settings.library_path.trim().is_empty() {
                None
            } else {
                Some(settings.library_path)
            }
        };
        if let Some(root) = library_root {
            if !Library::path_needs_config(&root) {
                remove_library_destination(Path::new(&root), dest).await?;
            }
        }
    }

    sqlx::query(
        r#"
        UPDATE downloads SET
            status = 'completed',
            destination_path = NULL,
            error_message = NULL,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

/// Pull live save/content paths from qBittorrent and requeue path-related pack failures.
/// Fixes existing installs stuck pointing at `/incomplete` after the torrent finished.
pub async fn refresh_qbittorrent(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    if settings.qbittorrent_url.is_empty() {
        return Err(AppError::BadRequest("qBittorrent is not configured".into()));
    }

    let torrents = state
        .qb
        .list_torrents(
            &settings.qbittorrent_url,
            &settings.qbittorrent_username,
            &settings.qbittorrent_password,
        )
        .await?;
    let torrent = torrents
        .iter()
        .find(|t| t.hash.eq_ignore_ascii_case(&download.info_hash))
        .ok_or_else(|| {
            AppError::BadRequest(
                "Torrent not found in qBittorrent — it may have been removed".into(),
            )
        })?;

    let result = apply_qbittorrent_refresh(&state.pool, &download, torrent).await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "ok": true,
        "save_path": result.save_path,
        "content_path": result.content_path,
        "progress": result.progress,
        "qb_state": result.qb_state,
        "requeued_items": result.requeued_items,
        "paths_changed": result.paths_changed,
        "download": attach_metadata(&state.pool, download).await?,
    })))
}

struct QbitRefreshResult {
    save_path: String,
    content_path: String,
    progress: f64,
    qb_state: String,
    requeued_items: u64,
    paths_changed: bool,
}

async fn apply_qbittorrent_refresh(
    pool: &sqlx::SqlitePool,
    download: &Download,
    torrent: &QbTorrent,
) -> AppResult<QbitRefreshResult> {
    let old_save = download.save_path.clone().unwrap_or_default();
    let old_content = download.content_path.clone().unwrap_or_default();
    let paths_changed = old_save != torrent.save_path || old_content != torrent.content_path;

    let mapped = map_state(&torrent.state, torrent.progress);
    let torrent_completed = mapped == "completed";

    let status = if matches!(
        download.status.as_str(),
        "awaiting_match" | "imported" | "copying"
    ) {
        download.status.clone()
    } else if download.kind == "pack"
        && torrent_completed
        && matches!(
            download.status.as_str(),
            "awaiting_map" | "partial" | "completed" | "error"
        )
    {
        // Keep pack mapping statuses; bump error packs back to completed so imports retry.
        if download.status == "error" {
            "completed".into()
        } else {
            download.status.clone()
        }
    } else if torrent_completed {
        "completed".into()
    } else {
        mapped.to_string()
    };

    let completed_at = if torrent_completed && download.completed_at.is_none() {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        download.completed_at.clone()
    };

    sqlx::query(
        r#"
        UPDATE downloads SET
            name = ?,
            progress = ?,
            download_speed = ?,
            eta = ?,
            save_path = ?,
            content_path = ?,
            status = ?,
            error_message = CASE WHEN ? = 'error' THEN ? ELSE NULL END,
            completed_at = COALESCE(?, completed_at),
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(&torrent.name)
    .bind(torrent.progress)
    .bind(torrent.dlspeed)
    .bind(torrent.eta)
    .bind(&torrent.save_path)
    .bind(&torrent.content_path)
    .bind(&status)
    .bind(mapped)
    .bind(format!("qBittorrent state: {}", torrent.state))
    .bind(completed_at)
    .bind(download.id)
    .execute(pool)
    .await?;

    let mut requeued_items = 0u64;
    if download.kind == "pack" {
        if torrent_completed {
            let pending = sqlx::query(
                "UPDATE download_items SET status = 'ready', updated_at = datetime('now') WHERE download_id = ? AND status = 'pending'",
            )
            .bind(download.id)
            .execute(pool)
            .await?
            .rows_affected();
            requeued_items += pending;
        }

        // Requeue failed imports after a path refresh (incomplete → complete).
        let failed = sqlx::query(
            r#"
            UPDATE download_items SET
                status = 'ready',
                error_message = NULL,
                updated_at = datetime('now')
            WHERE download_id = ?
              AND status = 'error'
              AND (
                error_message LIKE '%incomplete%'
                OR error_message LIKE '%Source not found%'
                OR error_message LIKE '%does not exist%'
                OR ? = 1
              )
            "#,
        )
        .bind(download.id)
        .bind(if paths_changed || torrent_completed {
            1
        } else {
            0
        })
        .execute(pool)
        .await?
        .rows_affected();
        requeued_items += failed;

        if requeued_items > 0
            && matches!(
                status.as_str(),
                "awaiting_map" | "partial" | "completed" | "imported" | "error"
            )
        {
            sqlx::query(
                "UPDATE downloads SET status = 'completed', updated_at = datetime('now') WHERE id = ?",
            )
            .bind(download.id)
            .execute(pool)
            .await?;
        }
    }

    Ok(QbitRefreshResult {
        save_path: torrent.save_path.clone(),
        content_path: torrent.content_path.clone(),
        progress: torrent.progress,
        qb_state: torrent.state.clone(),
        requeued_items,
        paths_changed,
    })
}

pub async fn map_item(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<MapItemRequest>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.kind != "pack" {
        return Err(AppError::BadRequest(
            "Only pack downloads support file mapping".into(),
        ));
    }
    if download.status == "awaiting_match" {
        return Err(AppError::BadRequest(
            "Start the pack download before mapping books".into(),
        ));
    }

    let source_paths = collect_map_paths(&body)?;
    let primary_path = source_paths[0].clone();

    let library_id = resolve_library_id(&state.pool, auth.user.id, body.library_id).await?;
    let m = body.match_data;
    let authors = serde_json::to_string(&m.authors).unwrap_or_else(|_| "[]".into());
    let narrators = serde_json::to_string(&m.narrators).unwrap_or_else(|_| "[]".into());

    let item_status = if matches!(
        download.status.as_str(),
        "completed" | "awaiting_map" | "partial" | "imported" | "error"
    ) || download.progress >= 1.0
    {
        "ready"
    } else {
        "pending"
    };

    // Reject if any path is already mapped on this download.
    for path in &source_paths {
        let taken: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM download_item_sources WHERE download_id = ? AND source_path = ?",
        )
        .bind(download.id)
        .bind(path)
        .fetch_optional(&state.pool)
        .await?;
        if taken.is_some() {
            return Err(AppError::Conflict(format!(
                "Path already mapped: {path}"
            )));
        }
        let legacy: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM download_items WHERE download_id = ? AND source_path = ?",
        )
        .bind(download.id)
        .bind(path)
        .fetch_optional(&state.pool)
        .await?;
        if legacy.is_some() {
            return Err(AppError::Conflict(format!(
                "Path already mapped: {path}"
            )));
        }
    }

    let result = sqlx::query(
        r#"
        INSERT INTO download_items (download_id, source_path, library_id, status)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(download.id)
    .bind(&primary_path)
    .bind(library_id)
    .bind(item_status)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Conflict("That path is already mapped".into())
        } else {
            AppError::from(e)
        }
    })?;
    let item_id = result.last_insert_rowid();

    for path in &source_paths {
        sqlx::query(
            r#"
            INSERT INTO download_item_sources (download_id, download_item_id, source_path)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(download.id)
        .bind(item_id)
        .bind(path)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                AppError::Conflict(format!("Path already mapped: {path}"))
            } else {
                AppError::from(e)
            }
        })?;
    }

    sqlx::query(
        r#"
        INSERT INTO book_metadata
            (download_id, download_item_id, asin, title, subtitle, authors, narrators, series, series_index, cover_url, description, region)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(download.id)
    .bind(item_id)
    .bind(&m.asin)
    .bind(&m.title)
    .bind(&m.subtitle)
    .bind(authors)
    .bind(narrators)
    .bind(&m.series)
    .bind(&m.series_index)
    .bind(&m.cover_url)
    .bind(&m.description)
    .bind(&m.region)
    .execute(&state.pool)
    .await?;

    // Bump pack status so worker notices newly ready items (including after a prior full import).
    if item_status == "ready" {
        sqlx::query(
            r#"
            UPDATE downloads SET
                status = CASE
                    WHEN status IN ('imported', 'awaiting_map', 'partial', 'completed') THEN 'completed'
                    ELSE status
                END,
                error_message = NULL,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(download.id)
        .execute(&state.pool)
        .await?;
    }

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

pub async fn unmap_item(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath((id, item_id)): AxumPath<(i64, i64)>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    let item = sqlx::query_as::<_, DownloadItem>(
        "SELECT * FROM download_items WHERE id = ? AND download_id = ?",
    )
    .bind(item_id)
    .bind(download.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    if item.status == "imported" || item.status == "copying" {
        return Err(AppError::BadRequest(
            "Imported pack items can't be unmapped".into(),
        ));
    }

    sqlx::query("DELETE FROM book_metadata WHERE download_item_id = ?")
        .bind(item.id)
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM download_items WHERE id = ?")
        .bind(item.id)
        .execute(&state.pool)
        .await?;

    let remaining: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status != 'imported'",
    )
    .bind(download.id)
    .fetch_one(&state.pool)
    .await?;
    let imported: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status = 'imported'",
    )
    .bind(download.id)
    .fetch_one(&state.pool)
    .await?;
    let new_status = if imported.0 > 0 && remaining.0 > 0 {
        "partial"
    } else if imported.0 > 0 {
        "imported"
    } else if matches!(
        download.status.as_str(),
        "queued" | "downloading" | "completed"
    ) {
        download.status.as_str()
    } else {
        "awaiting_map"
    };
    sqlx::query(
        "UPDATE downloads SET status = ?, error_message = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(new_status)
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

/// Delete the library copy of an imported pack item and clear its mapping so paths can be remapped.
pub async fn unimport_item(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath((id, item_id)): AxumPath<(i64, i64)>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.kind != "pack" {
        return Err(AppError::BadRequest(
            "Only pack downloads support un-import".into(),
        ));
    }
    let item = sqlx::query_as::<_, DownloadItem>(
        "SELECT * FROM download_items WHERE id = ? AND download_id = ?",
    )
    .bind(item_id)
    .bind(download.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    if item.status == "copying" {
        return Err(AppError::BadRequest(
            "Cannot un-import while a copy is in progress".into(),
        ));
    }
    if item.status != "imported" {
        return Err(AppError::BadRequest(
            "Only imported pack items can be un-imported".into(),
        ));
    }

    let library = sqlx::query_as::<_, Library>(
        "SELECT id, name, path, abs_id, abs_path, created_at FROM libraries WHERE id = ?",
    )
    .bind(item.library_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("Library for this item no longer exists".into()))?;

    if Library::path_needs_config(&library.path) {
        return Err(AppError::BadRequest(
            "Library has no container path set; cannot safely delete the imported copy".into(),
        ));
    }

    if let Some(dest) = item.destination_path.as_deref().filter(|s| !s.trim().is_empty()) {
        remove_library_destination(Path::new(&library.path), dest).await?;
    }

    sqlx::query("DELETE FROM book_metadata WHERE download_item_id = ?")
        .bind(item.id)
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM download_items WHERE id = ?")
        .bind(item.id)
        .execute(&state.pool)
        .await?;

    let remaining: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status != 'imported'",
    )
    .bind(download.id)
    .fetch_one(&state.pool)
    .await?;
    let imported: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status = 'imported'",
    )
    .bind(download.id)
    .fetch_one(&state.pool)
    .await?;
    let new_status = if imported.0 > 0 && remaining.0 > 0 {
        "partial"
    } else if imported.0 > 0 {
        "imported"
    } else if matches!(
        download.status.as_str(),
        "queued" | "downloading" | "completed"
    ) {
        download.status.as_str()
    } else if remaining.0 > 0 {
        "partial"
    } else {
        "awaiting_map"
    };
    sqlx::query(
        "UPDATE downloads SET status = ?, error_message = NULL, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(new_status)
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

pub async fn match_metadata(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<MatchRequest>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }

    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    if download.kind == "pack" {
        return Err(AppError::BadRequest(
            "This is a pack download. Use map to assign Audible matches per folder.".into(),
        ));
    }

    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    if settings.qbittorrent_url.is_empty() {
        return Err(AppError::BadRequest(
            "qBittorrent is not configured. Ask an administrator to finish setup.".into(),
        ));
    }

    let m = body.match_data;
    let authors = serde_json::to_string(&m.authors).unwrap_or_else(|_| "[]".into());
    let narrators = serde_json::to_string(&m.narrators).unwrap_or_else(|_| "[]".into());
    let library_id = resolve_library_id(&state.pool, auth.user.id, body.library_id).await?;

    check_active_torrent_limit(&state.pool, auth.user.id).await?;

    sqlx::query("DELETE FROM book_metadata WHERE download_id = ? AND download_item_id IS NULL")
        .bind(download.id)
        .execute(&state.pool)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO book_metadata
            (download_id, asin, title, subtitle, authors, narrators, series, series_index, cover_url, description, region)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(download.id)
    .bind(&m.asin)
    .bind(&m.title)
    .bind(&m.subtitle)
    .bind(authors)
    .bind(narrators)
    .bind(&m.series)
    .bind(&m.series_index)
    .bind(&m.cover_url)
    .bind(&m.description)
    .bind(&m.region)
    .execute(&state.pool)
    .await?;

    state
        .qb
        .add_magnet(
            &settings.qbittorrent_url,
            &settings.qbittorrent_username,
            &settings.qbittorrent_password,
            &download.magnet_uri,
            Some("audiobooks"),
        )
        .await?;

    sqlx::query(
        "UPDATE downloads SET status = 'queued', kind = 'single', name = COALESCE(name, ?), library_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&m.title)
    .bind(library_id)
    .bind(download.id)
    .execute(&state.pool)
    .await?;

    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download.id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "download": attach_metadata(&state.pool, download).await?
    })))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: AuthSession,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Json<Value>> {
    let download = if auth.user.is_root() {
        sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(auth.user.id)
            .fetch_optional(&state.pool)
            .await?
    }
    .ok_or(AppError::NotFound)?;

    if matches!(
        download.status.as_str(),
        "completed" | "copying" | "imported" | "awaiting_map" | "partial"
    ) {
        return Err(AppError::BadRequest(
            "Completed downloads can't be removed from Audiobooker while they need to seed. Let qBittorrent drop them at your ratio limit."
                .into(),
        ));
    }

    let imported_items: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status = 'imported'",
    )
    .bind(download.id)
    .fetch_one(&state.pool)
    .await?;
    if imported_items.0 > 0 {
        return Err(AppError::BadRequest(
            "Completed downloads can't be removed from Audiobooker while they need to seed. Let qBittorrent drop them at your ratio limit."
                .into(),
        ));
    }

    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    if !settings.qbittorrent_url.is_empty() {
        let _ = state
            .qb
            .delete_torrent(
                &settings.qbittorrent_url,
                &settings.qbittorrent_username,
                &settings.qbittorrent_password,
                &download.info_hash,
                false,
            )
            .await;
    }

    sqlx::query("DELETE FROM downloads WHERE id = ?")
        .bind(download.id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "ok": true })))
}