use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::AppResult;
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
    Ok(Json(json!({
        "subscribed": count.0 > 0,
        "subscriptions": count.0
    })))
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
