use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::magnet::parse_download_input;
use crate::metadata::MetadataMatch;
use crate::models::{BookMetadata, Download, DownloadWithMetadata, Settings, User};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateDownloadRequest {
    pub input: String,
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct MatchRequest {
    pub match_data: MetadataMatch,
    pub library_id: Option<i64>,
}

async fn attach_metadata(pool: &sqlx::SqlitePool, download: Download) -> AppResult<DownloadWithMetadata> {
    let meta = sqlx::query_as::<_, BookMetadata>(
        "SELECT * FROM book_metadata WHERE download_id = ?",
    )
    .bind(download.id)
    .fetch_optional(pool)
    .await?;
    Ok(DownloadWithMetadata {
        download,
        metadata: meta.map(Into::into),
    })
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
    Path(username): Path<String>,
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
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let download = sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(auth.user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
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
        INSERT INTO downloads (user_id, magnet_uri, info_hash, name, status)
        VALUES (?, ?, ?, ?, 'awaiting_match')
        "#,
    )
    .bind(auth.user.id)
    .bind(&parsed.magnet_uri)
    .bind(&parsed.info_hash)
    .bind(&parsed.name)
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

pub async fn match_metadata(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<i64>,
    Json(body): Json<MatchRequest>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }

    let download = sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(auth.user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

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

    // Resolve which Audiobookshelf library this import should land in.
    let allowed = sqlx::query_as::<_, (i64, String)>(
        r#"
        SELECT l.id, l.path FROM libraries l
        INNER JOIN user_libraries ul ON ul.library_id = l.id
        WHERE ul.user_id = ?
        ORDER BY l.name
        "#,
    )
    .bind(auth.user.id)
    .fetch_all(&state.pool)
    .await?;
    if allowed.is_empty() {
        return Err(AppError::BadRequest(
            "Your account has no libraries assigned. Ask an admin to grant library access.".into(),
        ));
    }
    let library_id = if let Some(id) = body.library_id {
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
        if crate::models::Library::path_needs_config(path) {
            return Err(AppError::BadRequest(
                "That library has no container path set. Ask an admin to set it under Settings → Libraries."
                    .into(),
            ));
        }
    }

    sqlx::query("DELETE FROM book_metadata WHERE download_id = ?")
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

    // Send to qBittorrent now that metadata is matched
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
        "UPDATE downloads SET status = 'queued', name = COALESCE(name, ?), library_id = ?, updated_at = datetime('now') WHERE id = ?",
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
    Path(id): Path<i64>,
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
