use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::files::{entries_from_disk, entries_from_qb_paths, resolve_download_source};
use crate::magnet::parse_download_input;
use crate::metadata::MetadataMatch;
use crate::models::{
    BookMetadata, Download, DownloadItem, DownloadItemWithMetadata, DownloadWithMetadata, Library,
    Settings, User,
};
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
    pub source_path: String,
    pub match_data: MetadataMatch,
    pub library_id: Option<i64>,
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
            let item_meta = sqlx::query_as::<_, BookMetadata>(
                "SELECT * FROM book_metadata WHERE download_item_id = ?",
            )
            .bind(item.id)
            .fetch_optional(pool)
            .await?;
            out.push(DownloadItemWithMetadata {
                item,
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
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, must_change_password, created_at, updated_at FROM users WHERE username = ? COLLATE NOCASE",
    )
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
    let download = load_user_download(&state.pool, auth.user.id, id).await?;
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    let mut entries = Vec::new();
    let mut source = "none";

    if !settings.qbittorrent_url.is_empty()
        && !matches!(download.status.as_str(), "awaiting_match" | "error")
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
    })))
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
    if matches!(
        download.status.as_str(),
        "awaiting_match" | "error"
    ) {
        return Err(AppError::BadRequest(
            "Start the pack download before mapping books".into(),
        ));
    }

    let source_path = body
        .source_path
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/");
    if source_path.is_empty() || source_path.contains("..") {
        return Err(AppError::BadRequest("Invalid source path".into()));
    }

    let library_id = resolve_library_id(&state.pool, auth.user.id, body.library_id).await?;
    let m = body.match_data;
    let authors = serde_json::to_string(&m.authors).unwrap_or_else(|_| "[]".into());
    let narrators = serde_json::to_string(&m.narrators).unwrap_or_else(|_| "[]".into());

    let item_status = if matches!(
        download.status.as_str(),
        "completed" | "awaiting_map" | "partial" | "imported"
    ) {
        "ready"
    } else {
        "pending"
    };

    let result = sqlx::query(
        r#"
        INSERT INTO download_items (download_id, source_path, library_id, status)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(download.id)
    .bind(&source_path)
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

    // Bump pack status so worker notices newly ready items on completed torrents.
    if item_status == "ready" && matches!(download.status.as_str(), "awaiting_map" | "partial" | "completed") {
        sqlx::query(
            "UPDATE downloads SET status = 'completed', updated_at = datetime('now') WHERE id = ? AND status IN ('awaiting_map', 'partial', 'completed')",
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

    sqlx::query("DELETE FROM download_items WHERE id = ?")
        .bind(item.id)
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