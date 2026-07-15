use std::path::Path;

use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
use std::str::FromStr;

use crate::error::{AppError, AppResult};

pub async fn connect(database_path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = database_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::internal(format!("Failed to create data dir: {e}")))?;
    }

    let options = SqliteConnectOptions::from_str(&format!(
        "sqlite://{}",
        database_path.display()
    ))
    .map_err(AppError::internal)?
    .create_if_missing(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .foreign_keys(true);

    let pool = SqlitePool::connect_with(options)
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
    ensure_column(pool, "downloads", "library_id", "INTEGER REFERENCES libraries(id)").await?;
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
    // In case 003 was applied before Abs path column existed on old DBs that
    // skipped the migration file name, ensure column still lands.
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
    sqlx::raw_sql(sql)
        .execute(pool)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
        .bind(version)
        .execute(pool)
        .await
        .map_err(AppError::internal)?;
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
            .map(|name| name == column)
            .unwrap_or(false)
    });
    if !has {
        sqlx::query(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {ddl_type}"
        ))
        .execute(pool)
        .await
        .map_err(AppError::internal)?;
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
    // Placeholder until an admin mounts a share and assigns a container path.
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
        WHERE u.role = 'user' AND l.name = 'Default'
        "#,
    )
    .execute(pool)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}
