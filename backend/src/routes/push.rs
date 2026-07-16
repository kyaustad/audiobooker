use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::models::{NotificationPrefs, USER_COLUMNS, User};
use crate::push::{PushSubscriptionRequest, ensure_vapid_keys, save_subscription};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct UnsubscribeRequest {
    pub endpoint: Option<String>,
}

pub async fn vapid_public(
    State(state): State<AppState>,
    _auth: AuthSession,
) -> AppResult<Json<Value>> {
    let (public, _) = ensure_vapid_keys(&state.pool).await?;
    Ok(Json(json!({ "vapid_public_key": public })))
}

pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<PushSubscriptionRequest>,
) -> AppResult<Json<Value>> {
    let (public, _) = ensure_vapid_keys(&state.pool).await?;
    save_subscription(&state.pool, auth.user.id, &body).await?;
    Ok(Json(json!({
        "ok": true,
        "vapid_public_key": public
    })))
}

pub async fn status(
    State(state): State<AppState>,
    auth: AuthSession,
) -> AppResult<Json<Value>> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM push_subscriptions WHERE user_id = ?",
    )
    .bind(auth.user.id)
    .fetch_one(&state.pool)
    .await?;

    let user = sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE id = ?"
    ))
    .bind(auth.user.id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({
        "subscribed": count.0 > 0,
        "subscriptions": count.0,
        "preferences": NotificationPrefs::from(&user),
    })))
}

pub async fn update_preferences(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<NotificationPrefs>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    sqlx::query(
        r#"
        UPDATE users SET
            notify_imported = ?,
            notify_download_finished = ?,
            notify_pack_ready = ?,
            notify_failures = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(body.notify_imported)
    .bind(body.notify_download_finished)
    .bind(body.notify_pack_ready)
    .bind(body.notify_failures)
    .bind(auth.user.id)
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "preferences": body })))
}

pub async fn unsubscribe(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<UnsubscribeRequest>,
) -> AppResult<Json<Value>> {
    if let Some(endpoint) = body.endpoint.filter(|s| !s.is_empty()) {
        sqlx::query("DELETE FROM push_subscriptions WHERE user_id = ? AND endpoint = ?")
            .bind(auth.user.id)
            .bind(endpoint)
            .execute(&state.pool)
            .await?;
    } else {
        sqlx::query("DELETE FROM push_subscriptions WHERE user_id = ?")
            .bind(auth.user.id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn test_push(
    State(state): State<AppState>,
    auth: AuthSession,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(crate::error::AppError::Forbidden);
    }
    ensure_vapid_keys(&state.pool).await?;
    let settings = sqlx::query_as::<_, crate::models::Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM push_subscriptions WHERE user_id = ?")
            .bind(auth.user.id)
            .fetch_one(&state.pool)
            .await?;
    if count.0 == 0 {
        return Err(crate::error::AppError::BadRequest(
            "Enable notifications first".into(),
        ));
    }
    crate::push::notify_user(
        &state.pool,
        &settings,
        auth.user.id,
        &crate::push::PushPayload {
            title: "Audiobooker test".into(),
            body: "Notifications are working.".into(),
            url: "/#/".into(),
            tag: Some("audiobooker-test".into()),
        },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}
