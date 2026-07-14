use axum::{Json, extract::State};
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::AppResult;
use crate::models::Settings;
use crate::push::{PushSubscriptionRequest, ensure_vapid_keys, save_subscription};
use crate::state::AppState;

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

    let _settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "ok": true,
        "vapid_public_key": public
    })))
}
