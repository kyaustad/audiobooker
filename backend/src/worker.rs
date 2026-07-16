use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::files::{build_library_relative_path, copy_completed, resolve_download_source};
use crate::models::{
    BookMetadata, BookMetadataPublic, Download, DownloadItem, Library, NotifyKind, Settings,
    USER_COLUMNS, User,
};
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

            if let Err(err) = crate::abs_users::maybe_periodic_sync(&pool).await {
                tracing::warn!(error = %err, "ABS user sync tick failed");
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
        WHERE status IN ('queued', 'downloading', 'completed', 'copying', 'awaiting_map', 'partial')
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
            if !matches!(
                download.status.as_str(),
                "queued" | "error" | "awaiting_map" | "partial" | "imported"
            ) {
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
                    NotifyKind::Failure,
                    "Download failed",
                    &format!(
                        "{} is no longer in qBittorrent",
                        display_name(&download)
                    ),
                )
                .await;
            }
            // Packs may still import mapped items from disk after qBit drops them —
            // only try when we already have content paths.
            if download.kind == "pack"
                && matches!(download.status.as_str(), "awaiting_map" | "partial" | "completed")
            {
                import_download(pool, &settings, download.id).await?;
            }
            continue;
        };

        if download.status == "copying" || download.status == "imported" {
            continue;
        }

        let mapped = map_state(&torrent.state, torrent.progress);
        let torrent_completed = mapped == "completed";

        // Preserve awaiting_map / partial until imports finish, unless still downloading.
        let status = if matches!(download.status.as_str(), "awaiting_map" | "partial")
            && torrent_completed
        {
            download.status.as_str()
        } else if torrent_completed {
            "completed"
        } else {
            mapped
        };

        let newly_completed =
            torrent_completed && matches!(download.status.as_str(), "queued" | "downloading");
        let newly_failed = mapped == "error" && download.status != "error";

        let completed_at = if torrent_completed && download.completed_at.is_none() {
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
                status = CASE
                    WHEN status = 'awaiting_match' THEN status
                    WHEN status IN ('awaiting_map', 'partial') AND ? = 'completed' THEN status
                    ELSE ?
                END,
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
        .bind(mapped)
        .bind(format!("qBittorrent state: {}", torrent.state))
        .bind(completed_at)
        .bind(download.id)
        .execute(pool)
        .await?;

        // Promote pending pack items to ready once the torrent finishes.
        if torrent_completed {
            sqlx::query(
                "UPDATE download_items SET status = 'ready', updated_at = datetime('now') WHERE download_id = ? AND status = 'pending'",
            )
            .bind(download.id)
            .execute(pool)
            .await?;
        }

        if newly_failed {
            push(
                pool,
                &settings,
                download.user_id,
                NotifyKind::Failure,
                "Download failed",
                &format!("{} failed in qBittorrent ({})", display_name(&download), torrent.state),
            )
            .await;
        }

        if newly_completed {
            if download.kind == "pack" {
                push(
                    pool,
                    &settings,
                    download.user_id,
                    NotifyKind::PackReady,
                    "Pack ready to map",
                    &format!(
                        "{} finished downloading — map books to import",
                        display_name(&download)
                    ),
                )
                .await;
            } else {
                push(
                    pool,
                    &settings,
                    download.user_id,
                    NotifyKind::DownloadFinished,
                    "Download finished",
                    &format!("{} finished downloading — importing…", display_name(&download)),
                )
                .await;
            }
        }

        if torrent_completed
            || matches!(download.status.as_str(), "awaiting_map" | "partial" | "completed")
        {
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

async fn push(
    pool: &SqlitePool,
    settings: &Settings,
    user_id: i64,
    kind: NotifyKind,
    title: &str,
    body: &str,
) {
    let prefs = match sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE id = ?"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(u)) => u.notification_prefs(),
        Ok(None) => return,
        Err(err) => {
            tracing::warn!(error = %err, "load notify prefs failed");
            return;
        }
    };
    if !prefs.allows(kind) {
        return;
    }

    let tag = match kind {
        NotifyKind::Imported => "audiobooker-imported",
        NotifyKind::DownloadFinished => "audiobooker-download",
        NotifyKind::PackReady => "audiobooker-pack",
        NotifyKind::Failure => "audiobooker-failure",
    };

    let payload = PushPayload {
        title: title.into(),
        body: body.into(),
        url: "/#/".into(),
        tag: Some(tag.into()),
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

    if download.kind == "pack" {
        return import_pack(pool, settings, download).await;
    }

    let meta = sqlx::query_as::<_, BookMetadata>(
        "SELECT * FROM book_metadata WHERE download_id = ? AND download_item_id IS NULL",
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
            NotifyKind::Failure,
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

    let library_root = library_root_for(pool, settings, download.library_id).await?;
    let Some(library_root) = library_root else {
        sqlx::query(
            r#"
            UPDATE downloads SET
                status = 'error',
                error_message = ?,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(
            "Library has no container path set. Open Settings → Libraries and assign the mount path for this library.",
        )
        .bind(download_id)
        .execute(pool)
        .await?;
        return Ok(());
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
                NotifyKind::Imported,
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
                NotifyKind::Failure,
                "Import failed",
                &format!("{} could not be copied into your library", public.title),
            )
            .await;
        }
    }

    Ok(())
}

async fn import_pack(
    pool: &SqlitePool,
    settings: &Settings,
    download: Download,
) -> anyhow::Result<()> {
    let download_id = download.id;
    let items = sqlx::query_as::<_, DownloadItem>(
        "SELECT * FROM download_items WHERE download_id = ? AND status IN ('ready', 'pending', 'error')",
    )
    .bind(download_id)
    .fetch_all(pool)
    .await?;

    let ready: Vec<_> = items
        .into_iter()
        .filter(|i| i.status == "ready" || i.status == "error")
        .collect();

    if ready.is_empty() {
        sqlx::query(
            "UPDATE downloads SET status = 'awaiting_map', error_message = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(download_id)
        .execute(pool)
        .await?;
        return Ok(());
    }

    let content_root = resolve_download_source(
        download.content_path.as_deref(),
        download.save_path.as_deref(),
        &settings.download_path,
    );

    for item in ready {
        if item.status == "imported" || item.status == "copying" {
            continue;
        }
        let meta = sqlx::query_as::<_, BookMetadata>(
            "SELECT * FROM book_metadata WHERE download_item_id = ?",
        )
        .bind(item.id)
        .fetch_optional(pool)
        .await?;
        let Some(meta) = meta else {
            continue;
        };

        sqlx::query(
            "UPDATE download_items SET status = 'copying', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(item.id)
        .execute(pool)
        .await?;

        let public = BookMetadataPublic::from(meta);
        let relative = build_library_relative_path(&settings.path_template, &public);
        let library_root = library_root_for(pool, settings, Some(item.library_id)).await?;
        let Some(library_root) = library_root else {
            sqlx::query(
                "UPDATE download_items SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind("Library has no container path set")
            .bind(item.id)
            .execute(pool)
            .await?;
            continue;
        };

        let source = join_source(&content_root, &item.source_path);
        match copy_completed(&source, Path::new(&library_root), &relative).await {
            Ok(dest) => {
                sqlx::query(
                    r#"
                    UPDATE download_items SET
                        status = 'imported',
                        destination_path = ?,
                        error_message = NULL,
                        imported_at = datetime('now'),
                        updated_at = datetime('now')
                    WHERE id = ?
                    "#,
                )
                .bind(dest.to_string_lossy().to_string())
                .bind(item.id)
                .execute(pool)
                .await?;
                push(
                    pool,
                    settings,
                    download.user_id,
                    NotifyKind::Imported,
                    "Audiobook imported",
                    &format!("{} is ready in your library", public.title),
                )
                .await;
            }
            Err(err) => {
                sqlx::query(
                    "UPDATE download_items SET status = 'error', error_message = ?, updated_at = datetime('now') WHERE id = ?",
                )
                .bind(err.to_string())
                .bind(item.id)
                .execute(pool)
                .await?;
                push(
                    pool,
                    settings,
                    download.user_id,
                    NotifyKind::Failure,
                    "Import failed",
                    &format!("{} could not be copied into your library", public.title),
                )
                .await;
            }
        }
    }

    let counts: (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM download_items WHERE download_id = ?),
            (SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status = 'imported'),
            (SELECT COUNT(*) FROM download_items WHERE download_id = ? AND status IN ('ready', 'pending', 'copying', 'error'))
        "#,
    )
    .bind(download_id)
    .bind(download_id)
    .bind(download_id)
    .fetch_one(pool)
    .await?;

    let (total, imported, remaining) = counts;
    let new_status = if total == 0 {
        "awaiting_map"
    } else if remaining == 0 && imported > 0 {
        "imported"
    } else if imported > 0 {
        "partial"
    } else {
        "awaiting_map"
    };

    sqlx::query(
        r#"
        UPDATE downloads SET
            status = ?,
            imported_at = CASE WHEN ? = 'imported' THEN datetime('now') ELSE imported_at END,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(new_status)
    .bind(new_status)
    .bind(download_id)
    .execute(pool)
    .await?;

    Ok(())
}

fn join_source(root: &Path, relative: &str) -> PathBuf {
    let rel = relative.trim_start_matches('/').replace('\\', "/");
    if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    }
}

async fn library_root_for(
    pool: &SqlitePool,
    settings: &Settings,
    library_id: Option<i64>,
) -> anyhow::Result<Option<String>> {
    if let Some(lib_id) = library_id {
        let row: Option<(String,)> = sqlx::query_as("SELECT path FROM libraries WHERE id = ?")
            .bind(lib_id)
            .fetch_optional(pool)
            .await?;
        match row.map(|r| r.0) {
            Some(path) if Library::path_needs_config(&path) => Ok(None),
            Some(path) => Ok(Some(path)),
            None => Ok(Some(settings.library_path.clone()).filter(|s| !s.trim().is_empty())),
        }
    } else {
        Ok(Some(settings.library_path.clone()).filter(|s| !s.trim().is_empty()))
    }
}
