use axum::{Json, extract::State};
use serde_json::{Value, json};

use crate::auth::{AuthSession, generate_api_key, hash_api_key};
use crate::error::AppResult;
use crate::state::AppState;

pub async fn info(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT key_prefix, created_at, rotated_at FROM api_keys WHERE id = 1",
    )
    .fetch_optional(&state.pool)
    .await?;

    let (prefix, created_at, rotated_at) = row.unwrap_or_default();
    Ok(Json(json!({
        "configured": !prefix.is_empty(),
        "key_prefix": prefix,
        "created_at": created_at,
        "rotated_at": rotated_at,
    })))
}

pub async fn rotate(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let raw = generate_api_key();
    let hash = hash_api_key(&raw);
    let prefix = raw.chars().take(12).collect::<String>();

    sqlx::query(
        r#"
        UPDATE api_keys SET
            key_hash = ?,
            key_prefix = ?,
            created_at = COALESCE(created_at, datetime('now')),
            rotated_at = datetime('now')
        WHERE id = 1
        "#,
    )
    .bind(hash)
    .bind(&prefix)
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({
        "api_key": raw,
        "key_prefix": prefix,
        "warning": "Copy this key now. It will not be shown again."
    })))
}
