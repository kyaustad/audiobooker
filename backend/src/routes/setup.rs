use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{hash_password, user_count};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
    pub qbittorrent_url: Option<String>,
    pub qbittorrent_username: Option<String>,
    pub qbittorrent_password: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct TestQbittorrentRequest {
    pub qbittorrent_url: Option<String>,
    pub qbittorrent_username: Option<String>,
    pub qbittorrent_password: Option<String>,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let count = user_count(&state.pool).await?;
    Ok(Json(json!({
        "needs_setup": count == 0,
        "user_count": count
    })))
}

pub async fn create_root(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> AppResult<Json<Value>> {
    let count = user_count(&state.pool).await?;
    if count > 0 {
        return Err(AppError::Conflict("Setup already completed".into()));
    }

    let username = body.username.trim().to_string();
    let password = body.password;
    if username.len() < 3 {
        return Err(AppError::BadRequest(
            "Username must be at least 3 characters".into(),
        ));
    }
    if password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    let hash = hash_password(&password)?;
    sqlx::query(
        r#"
        INSERT INTO users (username, password_hash, role, must_change_password)
        VALUES (?, ?, 'root', 0)
        "#,
    )
    .bind(&username)
    .bind(hash)
    .execute(&state.pool)
    .await?;

    if let Some(url) = body
        .qbittorrent_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let qbit_user = body
            .qbittorrent_username
            .as_deref()
            .unwrap_or("admin")
            .trim();
        let qbit_pass = body.qbittorrent_password.as_deref().unwrap_or("").to_string();
        sqlx::query(
            r#"
            UPDATE settings SET
                qbittorrent_url = ?,
                qbittorrent_username = ?,
                qbittorrent_password = ?,
                updated_at = datetime('now')
            WHERE id = 1
            "#,
        )
        .bind(url.trim_end_matches('/'))
        .bind(qbit_user)
        .bind(qbit_pass)
        .execute(&state.pool)
        .await?;
    }

    Ok(Json(json!({ "ok": true })))
}

/// Allows testing qBittorrent during first-boot setup before any user exists.
pub async fn test_qbittorrent(
    State(state): State<AppState>,
    Json(body): Json<TestQbittorrentRequest>,
) -> AppResult<Json<Value>> {
    let count = user_count(&state.pool).await?;
    if count > 0 {
        return Err(AppError::Forbidden);
    }

    let url = body
        .qbittorrent_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("qBittorrent URL is required".into()))?
        .trim_end_matches('/');
    let username = body
        .qbittorrent_username
        .as_deref()
        .unwrap_or("admin")
        .trim();
    let password = body.qbittorrent_password.as_deref().unwrap_or("");

    if username.is_empty() {
        return Err(AppError::BadRequest(
            "qBittorrent username is required for WebUI auth".into(),
        ));
    }

    state.qb.test_connection(url, username, password).await?;
    Ok(Json(json!({ "ok": true })))
}
