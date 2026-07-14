use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::models::{Settings, SettingsPublic};
use crate::push::ensure_vapid_keys;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub qbittorrent_url: Option<String>,
    pub qbittorrent_username: Option<String>,
    pub qbittorrent_password: Option<String>,
    pub download_path: Option<String>,
    pub library_path: Option<String>,
    pub path_template: Option<String>,
    pub audible_region: Option<String>,
    pub metadata_provider_url: Option<String>,
    pub sync_interval_ms: Option<i64>,
    pub vapid_subject: Option<String>,
}

pub async fn get(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "settings": SettingsPublic::from(settings) })))
}

pub async fn update(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<UpdateSettingsRequest>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let mut settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    if let Some(v) = body.qbittorrent_url {
        settings.qbittorrent_url = v.trim_end_matches('/').to_string();
    }
    if let Some(v) = body.qbittorrent_username {
        settings.qbittorrent_username = v;
    }
    if let Some(v) = body.qbittorrent_password {
        if !v.is_empty() {
            settings.qbittorrent_password = v;
        }
    }
    if let Some(v) = body.download_path {
        settings.download_path = v;
    }
    if let Some(v) = body.library_path {
        settings.library_path = v;
    }
    if let Some(v) = body.path_template {
        settings.path_template = v;
    }
    if let Some(v) = body.audible_region {
        settings.audible_region = v;
    }
    if let Some(v) = body.metadata_provider_url {
        settings.metadata_provider_url = v.trim_end_matches('/').to_string();
    }
    if let Some(v) = body.sync_interval_ms {
        settings.sync_interval_ms = v.max(3000);
    }
    if let Some(v) = body.vapid_subject {
        settings.vapid_subject = v;
    }

    sqlx::query(
        r#"
        UPDATE settings SET
            qbittorrent_url = ?,
            qbittorrent_username = ?,
            qbittorrent_password = ?,
            download_path = ?,
            library_path = ?,
            path_template = ?,
            audible_region = ?,
            metadata_provider_url = ?,
            sync_interval_ms = ?,
            vapid_subject = ?,
            updated_at = datetime('now')
        WHERE id = 1
        "#,
    )
    .bind(&settings.qbittorrent_url)
    .bind(&settings.qbittorrent_username)
    .bind(&settings.qbittorrent_password)
    .bind(&settings.download_path)
    .bind(&settings.library_path)
    .bind(&settings.path_template)
    .bind(&settings.audible_region)
    .bind(&settings.metadata_provider_url)
    .bind(settings.sync_interval_ms)
    .bind(&settings.vapid_subject)
    .execute(&state.pool)
    .await?;

    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "settings": SettingsPublic::from(settings) })))
}

pub async fn test_qbittorrent(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(payload): Json<TestQbittorrentRequest>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    let url = payload
        .qbittorrent_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(settings.qbittorrent_url.as_str())
        .trim_end_matches('/');

    if url.is_empty() {
        return Err(AppError::BadRequest("qBittorrent URL is not configured".into()));
    }

    let username = payload
        .qbittorrent_username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(settings.qbittorrent_username.as_str());

    // Prefer unsaved form password when provided; otherwise use saved password.
    let password = match payload.qbittorrent_password.as_deref() {
        Some(p) if !p.is_empty() => p,
        _ => settings.qbittorrent_password.as_str(),
    };

    if username.is_empty() {
        return Err(AppError::BadRequest(
            "qBittorrent username is required for WebUI auth".into(),
        ));
    }

    state.qb.test_connection(url, username, password).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
pub struct TestQbittorrentRequest {
    pub qbittorrent_url: Option<String>,
    pub qbittorrent_username: Option<String>,
    pub qbittorrent_password: Option<String>,
}

pub async fn ensure_vapid(
    State(state): State<AppState>,
    auth: AuthSession,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let (public, _) = ensure_vapid_keys(&state.pool).await?;
    Ok(Json(json!({ "vapid_public_key": public })))
}
