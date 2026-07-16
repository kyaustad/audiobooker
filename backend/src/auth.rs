use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    extract::{FromRequestParts, State},
    http::request::Parts,
};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{AuthUser, User},
    state::AppState,
};

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::internal(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    let parsed = PasswordHash::new(hash).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn hash_api_key(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn generate_api_key() -> String {
    format!("abk_{}", Uuid::new_v4().simple())
}

pub async fn create_session(pool: &SqlitePool, user_id: i64, ttl_hours: i64) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let expires = (Utc::now() + Duration::hours(ttl_hours)).to_rfc3339();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(user_id)
        .bind(expires)
        .execute(pool)
        .await?;
    Ok(id)
}

pub async fn destroy_session(pool: &SqlitePool, session_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn user_from_session(pool: &SqlitePool, session_id: &str) -> AppResult<Option<User>> {
    let now = Utc::now().to_rfc3339();
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT u.id, u.username, u.password_hash, u.role, u.must_change_password,
               u.notify_imported, u.notify_download_finished, u.notify_pack_ready, u.notify_failures,
               u.abs_user_id, u.created_at, u.updated_at
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.id = ? AND s.expires_at > ?
        "#,
    )
    .bind(session_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

pub async fn user_count(pool: &SqlitePool) -> AppResult<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

#[derive(Clone)]
pub struct AuthSession {
    pub user: User,
    pub session_id: Option<String>,
    pub via_api_key: bool,
}

impl AuthSession {
    pub fn auth_user(&self) -> AuthUser {
        self.user.clone().into()
    }

    pub fn require_root(&self) -> AppResult<()> {
        if self.user.is_root() {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub fn require_user_role(&self) -> AppResult<()> {
        if self.user.role == "user" {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for AuthSession {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(api_key) = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
        {
            if let Some(user) = authenticate_api_key(&state.pool, api_key).await? {
                return Ok(AuthSession {
                    user,
                    session_id: None,
                    via_api_key: true,
                });
            }
            return Err(AppError::Unauthorized);
        }

        let jar = CookieJar::from_headers(&parts.headers);
        let session_id = jar
            .get("audiobooker_session")
            .map(|c| c.value().to_string())
            .ok_or(AppError::Unauthorized)?;

        let user = user_from_session(&state.pool, &session_id)
            .await?
            .ok_or(AppError::Unauthorized)?;

        Ok(AuthSession {
            user,
            session_id: Some(session_id),
            via_api_key: false,
        })
    }
}

/// Optional auth for endpoints that work both ways.
pub struct OptionalAuth(pub Option<AuthSession>);

impl FromRequestParts<AppState> for OptionalAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match AuthSession::from_request_parts(parts, state).await {
            Ok(session) => Ok(OptionalAuth(Some(session))),
            Err(AppError::Unauthorized) => Ok(OptionalAuth(None)),
            Err(err) => Err(err),
        }
    }
}

async fn authenticate_api_key(pool: &SqlitePool, raw: &str) -> AppResult<Option<User>> {
    let hash = hash_api_key(raw);
    let row: Option<(String,)> = sqlx::query_as("SELECT key_hash FROM api_keys WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    let Some((stored,)) = row else {
        return Ok(None);
    };
    if stored.is_empty() || stored != hash {
        return Ok(None);
    }

    // API key authenticates as root for admin automation.
    let user = sqlx::query_as::<_, User>(&format!(
        "SELECT {cols} FROM users WHERE role = 'root' ORDER BY id LIMIT 1",
        cols = crate::models::USER_COLUMNS
    ))
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

pub fn session_cookie(value: &str, secure: bool, max_age_hours: i64) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};
    let mut cookie = Cookie::new("audiobooker_session", value.to_string());
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(time::Duration::hours(max_age_hours));
    if secure {
        cookie.set_secure(true);
    }
    cookie
}

pub fn clear_session_cookie(secure: bool) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};
    let mut cookie = Cookie::new("audiobooker_session", "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(time::Duration::seconds(0));
    if secure {
        cookie.set_secure(true);
    }
    cookie
}

#[allow(dead_code)]
pub async fn require_state(State(_state): State<AppState>) {}
