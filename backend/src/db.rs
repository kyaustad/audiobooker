use std::path::Path;

use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
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

    let migration = include_str!("../migrations/001_init.sql");
    sqlx::raw_sql(migration)
        .execute(&pool)
        .await
        .map_err(AppError::internal)?;

    Ok(pool)
}
