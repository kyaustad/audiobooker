use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::{Settings, USER_COLUMNS, User};

fn effective(user_override: Option<i64>, global: i64) -> i64 {
    user_override.unwrap_or(global)
}

/// Enforce create-request rate limit before inserting a download.
pub async fn check_request_limit(pool: &SqlitePool, user_id: i64) -> AppResult<()> {
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    let user = sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE id = ?"
    ))
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let limit = effective(user.rate_limit_requests, settings.rate_limit_requests);
    if limit <= 0 {
        return Ok(());
    }
    let window = effective(user.rate_limit_window_secs, settings.rate_limit_window_secs).max(1);
    let since = format!("-{window} seconds");

    let user_count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM downloads
        WHERE user_id = ?
          AND created_at >= datetime('now', ?)
        "#,
    )
    .bind(user_id)
    .bind(&since)
    .fetch_one(pool)
    .await?;

    if user_count.0 >= limit {
        return Err(AppError::TooManyRequests(format!(
            "Download request limit reached ({limit} per {window}s). Try again later."
        )));
    }

    // Optional global cap when settings limit is set (applies across all users).
    if settings.rate_limit_requests > 0 {
        let global_window = settings.rate_limit_window_secs.max(1);
        let global_since = format!("-{global_window} seconds");
        let global_count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM downloads
            WHERE created_at >= datetime('now', ?)
            "#,
        )
        .bind(&global_since)
        .fetch_one(pool)
        .await?;
        if global_count.0 >= settings.rate_limit_requests {
            return Err(AppError::TooManyRequests(format!(
                "Global download request limit reached ({} per {}s). Try again later.",
                settings.rate_limit_requests, global_window
            )));
        }
    }

    Ok(())
}

/// Enforce concurrent active torrent limit before adding to qBittorrent.
pub async fn check_active_torrent_limit(pool: &SqlitePool, user_id: i64) -> AppResult<()> {
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    let user = sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE id = ?"
    ))
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let limit = effective(
        user.rate_limit_active_torrents,
        settings.rate_limit_active_torrents,
    );
    if limit <= 0 {
        return Ok(());
    }

    let active: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM downloads
        WHERE user_id = ? AND status IN ('queued', 'downloading')
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    if active.0 >= limit {
        return Err(AppError::TooManyRequests(format!(
            "Active torrent limit reached ({limit}). Wait for a download to finish."
        )));
    }

    Ok(())
}
