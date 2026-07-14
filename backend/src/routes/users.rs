use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{AuthSession, hash_password};
use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
}

pub async fn list(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let users = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, must_change_password, created_at, updated_at FROM users ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await?;

    let items: Vec<Value> = users
        .into_iter()
        .map(|u| {
            json!({
                "id": u.id,
                "username": u.username,
                "role": u.role,
                "must_change_password": u.must_change_password,
                "created_at": u.created_at,
            })
        })
        .collect();
    Ok(Json(json!({ "users": items })))
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<CreateUserRequest>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let username = body.username.trim().to_string();
    if username.len() < 3 {
        return Err(AppError::BadRequest(
            "Username must be at least 3 characters".into(),
        ));
    }
    if body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    let exists: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
            .bind(&username)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Username already exists".into()));
    }

    let hash = hash_password(&body.password)?;
    let result = sqlx::query(
        r#"
        INSERT INTO users (username, password_hash, role, must_change_password)
        VALUES (?, ?, 'user', 1)
        "#,
    )
    .bind(&username)
    .bind(hash)
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({
        "user": {
            "id": result.last_insert_rowid(),
            "username": username,
            "role": "user",
            "must_change_password": true
        }
    })))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, must_change_password, created_at, updated_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    if user.is_root() {
        return Err(AppError::BadRequest("Cannot delete root user".into()));
    }

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
