use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::files::{build_library_relative_path, copy_completed, resolve_download_source};
use crate::models::{BookMetadata, BookMetadataPublic, Download, Settings};
use crate::push::{PushPayload, notify_user};
use crate::qbittorrent::{QbittorrentClient, map_state};

pub fn spawn_worker(pool: SqlitePool, qb: QbittorrentClient) {
    tokio::spawn(async move {
        loop {
            let interval_ms = match sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
                .fetch_one(&pool)
                .await
            {
                Ok(s) => s.sync_interval_ms.max(3000) as u64,
                Err(_) => 10000,
            };

            if let Err(err) = sync_once(&pool, &qb).await {
                tracing::warn!(error = %err, "sync worker tick failed");
            }

            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    });
}

async fn sync_once(pool: &SqlitePool, qb: &QbittorrentClient) -> anyhow::Result<()> {
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;

    if settings.qbittorrent_url.is_empty() {
        return Ok(());
    }

    let active = sqlx::query_as::<_, Download>(
        r#"
        SELECT * FROM downloads
        WHERE status IN ('queued', 'downloading', 'completed', 'copying')
        "#,
    )
    .fetch_all(pool)
    .await?;

    if active.is_empty() {
        return Ok(());
    }

    let torrents = qb
        .list_torrents(
            &settings.qbittorrent_url,
            &settings.qbittorrent_username,
            &settings.qbittorrent_password,
        )
        .await?;

    for download in active {
        let torrent = torrents
            .iter()
            .find(|t| t.hash.eq_ignore_ascii_case(&download.info_hash));

        let Some(torrent) = torrent else {
            if download.status != "queued" {
                sqlx::query(
                    "UPDATE downloads SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
                )
                .bind("Torrent not found in qBittorrent")
                .bind(download.id)
                .execute(pool)
                .await?;
            }
            continue;
        };

        // Don't overwrite importing states incorrectly
        if download.status == "copying" || download.status == "imported" {
            continue;
        }

        let mapped = map_state(&torrent.state, torrent.progress);
        let status = if mapped == "completed" {
            "completed"
        } else {
            mapped
        };

        let completed_at = if status == "completed" && download.completed_at.is_none() {
            Some(Utc::now().to_rfc3339())
        } else {
            download.completed_at.clone()
        };

        sqlx::query(
            r#"
            UPDATE downloads SET
                name = ?,
                progress = ?,
                download_speed = ?,
                eta = ?,
                save_path = ?,
                content_path = ?,
                status = CASE WHEN status = 'awaiting_match' THEN status ELSE ? END,
                error_message = CASE WHEN ? = 'error' THEN ? ELSE NULL END,
                completed_at = ?,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(&torrent.name)
        .bind(torrent.progress)
        .bind(torrent.dlspeed)
        .bind(torrent.eta)
        .bind(&torrent.save_path)
        .bind(&torrent.content_path)
        .bind(status)
        .bind(status)
        .bind(format!("qBittorrent state: {}", torrent.state))
        .bind(completed_at)
        .bind(download.id)
        .execute(pool)
        .await?;

        if status == "completed" {
            import_download(pool, &settings, download.id).await?;
        }
    }

    Ok(())
}

async fn import_download(pool: &SqlitePool, settings: &Settings, download_id: i64) -> anyhow::Result<()> {
    let download = sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(download_id)
        .fetch_one(pool)
        .await?;

    if download.status == "imported" || download.status == "copying" {
        return Ok(());
    }

    let meta = sqlx::query_as::<_, BookMetadata>(
        "SELECT * FROM book_metadata WHERE download_id = ?",
    )
    .bind(download_id)
    .fetch_optional(pool)
    .await?;

    let Some(meta) = meta else {
        sqlx::query(
            "UPDATE downloads SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind("Download completed but no Audible metadata matched")
        .bind(download_id)
        .execute(pool)
        .await?;
        return Ok(());
    };

    sqlx::query(
        "UPDATE downloads SET status = 'copying', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(download_id)
    .execute(pool)
    .await?;

    let public = BookMetadataPublic::from(meta);
    let relative = build_library_relative_path(&settings.path_template, &public);

    let source_path = resolve_download_source(
        download.content_path.as_deref(),
        download.save_path.as_deref(),
        &settings.download_path,
    );
    match copy_completed(&source_path, Path::new(&settings.library_path), &relative).await {
        Ok(dest) => {
            sqlx::query(
                r#"
                UPDATE downloads SET
                    status = 'imported',
                    destination_path = ?,
                    imported_at = datetime('now'),
                    error_message = NULL,
                    updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(dest.to_string_lossy().to_string())
            .bind(download_id)
            .execute(pool)
            .await?;

            let payload = PushPayload {
                title: "Audiobook imported".into(),
                body: format!("{} is ready in your library", public.title),
                url: "/".into(),
            };
            let _ = notify_user(pool, settings, download.user_id, &payload).await;
        }
        Err(err) => {
            sqlx::query(
                "UPDATE downloads SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(err.to_string())
            .bind(download_id)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}
