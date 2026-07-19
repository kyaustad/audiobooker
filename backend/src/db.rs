use std::path::Path;
use std::time::Duration;

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous},
};
use std::str::FromStr;

use crate::error::{AppError, AppResult};

pub async fn connect(database_path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = database_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::internal(format!("Failed to create data dir: {e}")))?;
    }

    // SQLite allows many concurrent readers (WAL) but only ONE writer at a time.
    // A multi-connection pool makes writers fight over the DB lock (SQLITE_BUSY /
    // multi-second "slow statement" waits) when the API + background worker overlap.
    //
    // max_connections(1) turns the pool into an application-level queue: handlers and
    // the worker wait for the connection instead of contending inside SQLite. That is
    // the right model for a handful of users (≈4–5) on Unraid/Docker.
    //
    // busy_timeout remains as a safety net if anything else opens the same file.
    let options = SqliteConnectOptions::from_str(&format!(
        "sqlite://{}",
        database_path.display()
    ))
    .map_err(AppError::internal)?
    .create_if_missing(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .synchronous(SqliteSynchronous::Normal)
    .busy_timeout(Duration::from_secs(10))
    .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(None)
        .connect_with(options)
        .await
        .map_err(AppError::internal)?;

    // Extra PRAGMAs on the live connection set.
    sqlx::query("PRAGMA temp_store = MEMORY;")
        .execute(&pool)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("PRAGMA wal_autocheckpoint = 1000;")
        .execute(&pool)
        .await
        .map_err(AppError::internal)?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(AppError::internal)?;

    run_once(pool, "001_init", include_str!("../migrations/001_init.sql")).await?;
    run_once(pool, "002_libraries", include_str!("../migrations/002_libraries.sql")).await?;
    run_once(
        pool,
        "003_library_abs_path",
        include_str!("../migrations/003_library_abs_path.sql"),
    )
    .await?;
    run_once(
        pool,
        "004_pack_items",
        include_str!("../migrations/004_pack_items.sql"),
    )
    .await?;
    run_once(
        pool,
        "005_notification_prefs",
        include_str!("../migrations/005_notification_prefs.sql"),
    )
    .await?;
    run_once(
        pool,
        "006_abs_user_sync",
        include_str!("../migrations/006_abs_user_sync.sql"),
    )
    .await?;
    run_once(
        pool,
        "007_item_sources",
        include_str!("../migrations/007_item_sources.sql"),
    )
    .await?;
    run_once(
        pool,
        "008_rate_limits",
        include_str!("../migrations/008_rate_limits.sql"),
    )
    .await?;
    run_once(
        pool,
        "009_roles_approvals",
        include_str!("../migrations/009_roles_approvals.sql"),
    )
    .await?;
    ensure_column(pool, "downloads", "library_id", "INTEGER REFERENCES libraries(id)").await?;
    ensure_column(pool, "downloads", "kind", "TEXT NOT NULL DEFAULT 'single'").await?;
    ensure_column(pool, "downloads", "map_files", "INTEGER NOT NULL DEFAULT 0").await?;
    ensure_column(pool, "users", "can_remove", "INTEGER NOT NULL DEFAULT 1").await?;
    ensure_column(pool, "users", "can_remove_files", "INTEGER NOT NULL DEFAULT 0").await?;
    ensure_column(pool, "users", "notify_imported", "INTEGER NOT NULL DEFAULT 1").await?;
    ensure_column(
        pool,
        "users",
        "notify_download_finished",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(pool, "users", "notify_pack_ready", "INTEGER NOT NULL DEFAULT 1").await?;
    ensure_column(pool, "users", "notify_failures", "INTEGER NOT NULL DEFAULT 1").await?;
    ensure_column(pool, "users", "abs_user_id", "TEXT").await?;
    ensure_column(pool, "users", "rate_limit_requests", "INTEGER").await?;
    ensure_column(pool, "users", "rate_limit_window_secs", "INTEGER").await?;
    ensure_column(pool, "users", "rate_limit_active_torrents", "INTEGER").await?;
    ensure_column(
        pool,
        "settings",
        "audiobookshelf_url",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "audiobookshelf_token",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "abs_user_sync_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "abs_user_sync_interval_ms",
        "INTEGER NOT NULL DEFAULT 3600000",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "abs_user_default_password",
        "TEXT NOT NULL DEFAULT 'changeme'",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "abs_user_sync_libraries",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    ensure_column(pool, "settings", "abs_user_last_sync_at", "TEXT").await?;
    ensure_column(
        pool,
        "settings",
        "rate_limit_requests",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "rate_limit_window_secs",
        "INTEGER NOT NULL DEFAULT 86400",
    )
    .await?;
    ensure_column(
        pool,
        "settings",
        "rate_limit_active_torrents",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(pool, "libraries", "abs_path", "TEXT").await?;
    seed_default_library(pool).await?;
    Ok(())
}

async fn run_once(pool: &SqlitePool, version: &str, sql: &str) -> AppResult<()> {
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
            .bind(version)
            .fetch_optional(pool)
            .await
            .map_err(AppError::internal)?;
    if exists.is_some() {
        return Ok(());
    }
    // Strip full-line `--` comments *before* splitting on `;`, otherwise a
    // semicolon inside a comment becomes a fake statement boundary
    // (e.g. `-- … in place; rebuild users` → `rebuild users`).
    let without_line_comments: String = sql
        .lines()
        .filter(|l| !l.trim().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");

    // Execute statements individually so a multi-statement migration cannot be
    // marked applied after only the first ALTER/CREATE succeeds.
    for stmt in without_line_comments.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        sqlx::raw_sql(stmt)
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("migration {version} failed: {e}")))?;
    }
    sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
        .bind(version)
        .execute(pool)
        .await
        .map_err(AppError::internal)?;
    tracing::info!(version, "applied database migration");
    Ok(())
}

async fn ensure_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    ddl_type: &str,
) -> AppResult<()> {
    let infos = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(AppError::internal)?;
    let has = infos.iter().any(|row| {
        row.try_get::<String, _>("name")
            .or_else(|_| row.try_get::<String, _>(1))
            .map(|name| name == column)
            .unwrap_or(false)
    });
    if !has {
        sqlx::query(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {ddl_type}"
        ))
        .execute(pool)
        .await
        .map_err(|e| {
            AppError::internal(format!("failed to add column {table}.{column}: {e}"))
        })?;
        tracing::info!(table, column, "added missing database column");
    }
    Ok(())
}

async fn seed_default_library(pool: &SqlitePool) -> AppResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM libraries")
        .fetch_one(pool)
        .await
        .map_err(AppError::internal)?;
    if count.0 > 0 {
        return Ok(());
    }
    sqlx::query("INSERT INTO libraries (name, path) VALUES (?, ?)")
        .bind("Default")
        .bind("__unset__/default")
        .execute(pool)
        .await
        .map_err(AppError::internal)?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO user_libraries (user_id, library_id)
        SELECT u.id, l.id FROM users u
        CROSS JOIN libraries l
        WHERE u.role IN ('user', 'requester', 'approver') AND l.name = 'Default'
        "#,
    )
    .execute(pool)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}
