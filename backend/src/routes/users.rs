use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{AuthSession, hash_password};
use crate::error::{AppError, AppResult};
use crate::models::{Library, User};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub library_ids: Option<Vec<i64>>,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub password: Option<String>,
    pub library_ids: Option<Vec<i64>>,
    pub must_change_password: Option<bool>,
}

async fn libraries_for_user(pool: &sqlx::SqlitePool, user_id: i64) -> AppResult<Vec<Library>> {
    let libraries = sqlx::query_as::<_, Library>(
        r#"
        SELECT l.id, l.name, l.path, l.abs_id, l.created_at
        FROM libraries l
        INNER JOIN user_libraries ul ON ul.library_id = l.id
        WHERE ul.user_id = ?
        ORDER BY l.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(libraries)
}

async fn set_user_libraries(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    library_ids: &[i64],
) -> AppResult<()> {
    if library_ids.is_empty() {
        return Err(AppError::BadRequest(
            "Assign at least one library to the user".into(),
        ));
    }
    for id in library_ids {
        let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM libraries WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Err(AppError::BadRequest(format!("Unknown library id {id}")));
        }
    }
    sqlx::query("DELETE FROM user_libraries WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    for id in library_ids {
        sqlx::query("INSERT INTO user_libraries (user_id, library_id) VALUES (?, ?)")
            .bind(user_id)
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn list(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let users = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, must_change_password, created_at, updated_at FROM users ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut items = Vec::new();
    for u in users {
        let libraries = if u.is_root() {
            Vec::new()
        } else {
            libraries_for_user(&state.pool, u.id).await?
        };
        items.push(json!({
            "id": u.id,
            "username": u.username,
            "role": u.role,
            "must_change_password": u.must_change_password,
            "created_at": u.created_at,
            "libraries": libraries,
            "library_ids": libraries.iter().map(|l| l.id).collect::<Vec<_>>(),
        }));
    }
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

    let library_ids = if let Some(ids) = body.library_ids.filter(|v| !v.is_empty()) {
        ids
    } else {
        // Default: all libraries
        let all: Vec<(i64,)> = sqlx::query_as("SELECT id FROM libraries ORDER BY id")
            .fetch_all(&state.pool)
            .await?;
        all.into_iter().map(|(id,)| id).collect()
    };
    if library_ids.is_empty() {
        return Err(AppError::BadRequest(
            "Create at least one library in Settings before adding users".into(),
        ));
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

    let user_id = result.last_insert_rowid();
    set_user_libraries(&state.pool, user_id, &library_ids).await?;
    let libraries = libraries_for_user(&state.pool, user_id).await?;

    Ok(Json(json!({
        "user": {
            "id": user_id,
            "username": username,
            "role": "user",
            "must_change_password": true,
            "libraries": libraries,
            "library_ids": library_ids,
        }
    })))
}

pub async fn update(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<i64>,
    Json(body): Json<UpdateUserRequest>,
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
        return Err(AppError::BadRequest("Cannot edit root user here".into()));
    }

    if let Some(password) = body.password.filter(|p| !p.is_empty()) {
        if password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }
        let hash = hash_password(&password)?;
        sqlx::query(
            "UPDATE users SET password_hash = ?, must_change_password = COALESCE(?, must_change_password), updated_at = datetime('now') WHERE id = ?",
        )
        .bind(hash)
        .bind(body.must_change_password)
        .bind(id)
        .execute(&state.pool)
        .await?;
    } else if let Some(flag) = body.must_change_password {
        sqlx::query(
            "UPDATE users SET must_change_password = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(flag)
        .bind(id)
        .execute(&state.pool)
        .await?;
    }

    if let Some(library_ids) = body.library_ids {
        set_user_libraries(&state.pool, id, &library_ids).await?;
    }

    let libraries = libraries_for_user(&state.pool, id).await?;
    Ok(Json(json!({
        "user": {
            "id": user.id,
            "username": user.username,
            "role": user.role,
            "must_change_password": body.must_change_password.unwrap_or(user.must_change_password),
            "libraries": libraries,
            "library_ids": libraries.iter().map(|l| l.id).collect::<Vec<_>>(),
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
