use axum::{
    Json,
    extract::State,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{
    AuthSession, clear_session_cookie, create_session, destroy_session, hash_password,
    session_cookie, verify_password,
};
use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct PasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let username = body.username.trim();
    let user = sqlx::query_as::<_, User>(&format!(
        "SELECT {cols} FROM users WHERE username = ? COLLATE NOCASE",
        cols = crate::models::USER_COLUMNS
    ))
    .bind(username)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    if !verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }

    let session_id =
        create_session(&state.pool, user.id, state.config.session_ttl_hours).await?;
    let jar = jar.add(session_cookie(
        &session_id,
        state.config.cookie_secure,
        state.config.session_ttl_hours,
    ));

    Ok((
        jar,
        Json(json!({ "user": AuthSession { user, session_id: Some(session_id), via_api_key: false }.auth_user() })),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthSession,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<Value>)> {
    if let Some(id) = auth.session_id {
        destroy_session(&state.pool, &id).await?;
    }
    let jar = jar.add(clear_session_cookie(state.config.cookie_secure));
    Ok((jar, Json(json!({ "ok": true }))))
}

pub async fn me(auth: AuthSession) -> Json<Value> {
    Json(json!({ "user": auth.auth_user() }))
}

pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<PasswordRequest>,
) -> AppResult<Json<Value>> {
    if !verify_password(&body.current_password, &auth.user.password_hash)? {
        return Err(AppError::BadRequest("Current password is incorrect".into()));
    }
    if body.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "New password must be at least 8 characters".into(),
        ));
    }
    let hash = hash_password(&body.new_password)?;
    sqlx::query(
        "UPDATE users SET password_hash = ?, must_change_password = 0, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(hash)
    .bind(auth.user.id)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}
