//! Sync Audiobookshelf users into Audiobooker.
//!
//! ABS never exposes passwords. New users get the configured default password
//! (default `changeme`) and `must_change_password = 1`.

use chrono::Utc;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::auth::hash_password;
use crate::error::{AppError, AppResult};
use crate::models::{Settings, USER_COLUMNS, User};

#[derive(Debug, Default, Clone)]
pub struct AbsUserSyncResult {
    pub created: usize,
    pub linked: usize,
    pub updated_libraries: usize,
    pub skipped: usize,
    pub total_abs_users: usize,
}

pub async fn sync_abs_users(pool: &SqlitePool, settings: &Settings) -> AppResult<AbsUserSyncResult> {
    let base = settings.audiobookshelf_url.trim().trim_end_matches('/');
    let token = settings.audiobookshelf_token.trim();
    if base.is_empty() || token.is_empty() {
        return Err(AppError::BadRequest(
            "Configure Audiobookshelf URL and API token first".into(),
        ));
    }

    let password = settings.abs_user_default_password.trim();
    let password = if password.is_empty() {
        "changeme"
    } else {
        password
    };
    if password.len() < 8 {
        return Err(AppError::BadRequest(
            "ABS sync default password must be at least 8 characters".into(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(AppError::internal)?;

    let url = format!("{base}/api/users");
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("ABS users request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "ABS users API returned {status}: {body}"
        )));
    }

    let payload: Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("ABS users JSON parse failed: {e}")))?;

    let users = payload
        .get("users")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut result = AbsUserSyncResult {
        total_abs_users: users.len(),
        ..Default::default()
    };

    let hash = hash_password(password)?;

    // All local libraries with an ABS id, for mapping.
    let abs_libs: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, abs_id FROM libraries WHERE abs_id IS NOT NULL AND abs_id != ''")
            .fetch_all(pool)
            .await?;

    let all_library_ids: Vec<i64> = sqlx::query_as::<_, (i64,)>("SELECT id FROM libraries ORDER BY id")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect();

    if all_library_ids.is_empty() {
        return Err(AppError::BadRequest(
            "Create or sync at least one library before syncing users".into(),
        ));
    }

    for abs_user in &users {
        let abs_id = abs_user
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let username = abs_user
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let user_type = abs_user
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        let is_active = abs_user
            .get("isActive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if abs_id.is_empty() || username.is_empty() {
            result.skipped += 1;
            continue;
        }
        // Only sync regular ABS users (not root/admin/guest).
        if user_type != "user" || !is_active {
            result.skipped += 1;
            continue;
        }

        let library_ids = if settings.abs_user_sync_libraries {
            resolve_library_ids(abs_user, &abs_libs, &all_library_ids)
        } else {
            all_library_ids.clone()
        };
        if library_ids.is_empty() {
            result.skipped += 1;
            continue;
        }

        // Prefer match by ABS id, then by username.
        let existing: Option<User> = if !abs_id.is_empty() {
            sqlx::query_as::<_, User>(&format!(
                "SELECT {USER_COLUMNS} FROM users WHERE abs_user_id = ?"
            ))
            .bind(abs_id)
            .fetch_optional(pool)
            .await?
        } else {
            None
        };

        let existing = match existing {
            Some(u) => Some(u),
            None => {
                sqlx::query_as::<_, User>(&format!(
                    "SELECT {USER_COLUMNS} FROM users WHERE username = ? COLLATE NOCASE"
                ))
                .bind(username)
                .fetch_optional(pool)
                .await?
            }
        };

        if let Some(user) = existing {
            if user.is_root() {
                result.skipped += 1;
                continue;
            }
            if user.abs_user_id.as_deref() != Some(abs_id) {
                sqlx::query(
                    "UPDATE users SET abs_user_id = ?, updated_at = datetime('now') WHERE id = ?",
                )
                .bind(abs_id)
                .bind(user.id)
                .execute(pool)
                .await?;
                result.linked += 1;
            }
            if settings.abs_user_sync_libraries {
                set_user_libraries_quiet(pool, user.id, &library_ids).await?;
                result.updated_libraries += 1;
            }
            continue;
        }

        let insert = sqlx::query(
            r#"
            INSERT INTO users (
                username, password_hash, role, must_change_password, abs_user_id
            ) VALUES (?, ?, 'user', 1, ?)
            "#,
        )
        .bind(username)
        .bind(&hash)
        .bind(abs_id)
        .execute(pool)
        .await?;

        let user_id = insert.last_insert_rowid();
        set_user_libraries_quiet(pool, user_id, &library_ids).await?;
        result.created += 1;
    }

    sqlx::query(
        "UPDATE settings SET abs_user_last_sync_at = ?, updated_at = datetime('now') WHERE id = 1",
    )
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;

    Ok(result)
}

fn resolve_library_ids(
    abs_user: &Value,
    abs_libs: &[(i64, String)],
    all_library_ids: &[i64],
) -> Vec<i64> {
    let access_all = abs_user
        .pointer("/permissions/accessAllLibraries")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if access_all {
        return all_library_ids.to_vec();
    }

    let accessible = abs_user
        .get("librariesAccessible")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if accessible.is_empty() {
        return all_library_ids.to_vec();
    }

    let mut ids = Vec::new();
    for entry in accessible {
        let abs_id = entry.as_str().unwrap_or("");
        if abs_id.is_empty() {
            continue;
        }
        if let Some((id, _)) = abs_libs.iter().find(|(_, a)| a == abs_id) {
            ids.push(*id);
        }
    }
    if ids.is_empty() {
        all_library_ids.to_vec()
    } else {
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

async fn set_user_libraries_quiet(
    pool: &SqlitePool,
    user_id: i64,
    library_ids: &[i64],
) -> AppResult<()> {
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

/// Called from the background worker when periodic sync is due.
pub async fn maybe_periodic_sync(pool: &SqlitePool) -> AppResult<Option<AbsUserSyncResult>> {
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    if !settings.abs_user_sync_enabled {
        return Ok(None);
    }
    if settings.audiobookshelf_url.trim().is_empty() || settings.audiobookshelf_token.trim().is_empty()
    {
        return Ok(None);
    }

    let interval = settings.abs_user_sync_interval_ms.max(60_000) as i64;
    if let Some(last) = settings.abs_user_last_sync_at.as_deref() {
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(last) {
            let elapsed = Utc::now().signed_duration_since(ts.with_timezone(&Utc));
            if elapsed.num_milliseconds() < interval {
                return Ok(None);
            }
        }
    }

    let result = sync_abs_users(pool, &settings).await?;
    tracing::info!(
        created = result.created,
        linked = result.linked,
        updated_libraries = result.updated_libraries,
        skipped = result.skipped,
        "ABS user sync complete"
    );
    Ok(Some(result))
}
