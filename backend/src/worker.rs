use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::files::{build_library_relative_path, copy_completed, resolve_download_source};
use crate::models::{BookMetadata, BookMetadataPublic, Download, Settings};
use crate::push::{PushPayload, ensure_vapid_keys, notify_user};
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
    let _ = ensure_vapid_keys(pool).await;
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
            if download.status != "queued" && download.status != "error" {
                sqlx::query(
                    "UPDATE downloads SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
                )
                .bind("Torrent not found in qBittorrent")
                .bind(download.id)
                .execute(pool)
                .await?;
                push(
                    pool,
                    &settings,
                    download.user_id,
                    "Download failed",
                    &format!(
                        "{} is no longer in qBittorrent",
                        display_name(&download)
                    ),
                )
                .await;
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

        let newly_completed =
            status == "completed" && matches!(download.status.as_str(), "queued" | "downloading");
        let newly_failed = status == "error" && download.status != "error";

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

        if newly_failed {
            push(
                pool,
                &settings,
                download.user_id,
                "Download failed",
                &format!("{} failed in qBittorrent ({})", display_name(&download), torrent.state),
            )
            .await;
        }

        if newly_completed {
            push(
                pool,
                &settings,
                download.user_id,
                "Download finished",
                &format!("{} finished downloading — importing…", display_name(&download)),
            )
            .await;
        }

        if status == "completed" {
            import_download(pool, &settings, download.id).await?;
        }
    }

    Ok(())
}

fn display_name(download: &Download) -> String {
    download
        .name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Your audiobook".into())
}

async fn push(pool: &SqlitePool, settings: &Settings, user_id: i64, title: &str, body: &str) {
    let payload = PushPayload {
        title: title.into(),
        body: body.into(),
        url: "/#/".into(),
    };
    if let Err(err) = notify_user(pool, settings, user_id, &payload).await {
        tracing::warn!(error = %err, %title, "push notify failed");
    }
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
        push(
            pool,
            settings,
            download.user_id,
            "Import failed",
            &format!(
                "{} finished downloading but has no Audible match",
                display_name(&download)
            ),
        )
        .await;
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

    let library_root = if let Some(lib_id) = download.library_id {
        let row: Option<(String,)> = sqlx::query_as("SELECT path FROM libraries WHERE id = ?")
            .bind(lib_id)
            .fetch_optional(pool)
            .await?;
        row.map(|r| r.0)
            .unwrap_or_else(|| settings.library_path.clone())
    } else {
        settings.library_path.clone()
    };

    let source_path = resolve_download_source(
        download.content_path.as_deref(),
        download.save_path.as_deref(),
        &settings.download_path,
    );
    match copy_completed(&source_path, Path::new(&library_root), &relative).await {
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

            push(
                pool,
                settings,
                download.user_id,
                "Audiobook imported",
                &format!("{} is ready in your library", public.title),
            )
            .await;
        }
        Err(err) => {
            sqlx::query(
                "UPDATE downloads SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(err.to_string())
            .bind(download_id)
            .execute(pool)
            .await?;

            push(
                pool,
                settings,
                download.user_id,
                "Import failed",
                &format!("{} could not be copied into your library", public.title),
            )
            .await;
        }
    }

    Ok(())
}
