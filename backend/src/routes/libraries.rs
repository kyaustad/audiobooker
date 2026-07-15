use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::models::{Library, Settings};
use crate::state::AppState;

const LIBRARY_SELECT: &str =
    "SELECT id, name, path, abs_id, abs_path, created_at FROM libraries";

#[derive(Deserialize)]
pub struct UpsertLibraryRequest {
    pub name: String,
    pub path: String,
    pub abs_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SyncAbsRequest {
    pub audiobookshelf_url: Option<String>,
    pub audiobookshelf_token: Option<String>,
}

fn unset_path_for(abs_id: &str, name: &str) -> String {
    if !abs_id.is_empty() {
        format!("__unset__/{abs_id}")
    } else {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        format!("__unset__/{slug}")
    }
}

pub async fn list_all(State(state): State<AppState>, auth: AuthSession) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let libraries = sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} ORDER BY name"))
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(json!({ "libraries": libraries })))
}

pub async fn list_for_me(
    State(state): State<AppState>,
    auth: AuthSession,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let libraries = sqlx::query_as::<_, Library>(
        r#"
        SELECT l.id, l.name, l.path, l.abs_id, l.abs_path, l.created_at
        FROM libraries l
        INNER JOIN user_libraries ul ON ul.library_id = l.id
        WHERE ul.user_id = ?
        ORDER BY l.name
        "#,
    )
    .bind(auth.user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "libraries": libraries })))
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<UpsertLibraryRequest>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let name = body.name.trim().to_string();
    let path = body.path.trim().trim_end_matches('/').to_string();
    if name.is_empty() || path.is_empty() {
        return Err(AppError::BadRequest("Name and path are required".into()));
    }
    if Library::path_needs_config(&path) {
        return Err(AppError::BadRequest(
            "Set a real container path (the mount point inside this container)".into(),
        ));
    }
    let result = sqlx::query(
        "INSERT INTO libraries (name, path, abs_id) VALUES (?, ?, ?)",
    )
    .bind(&name)
    .bind(&path)
    .bind(body.abs_id.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Conflict("Library name or path already exists".into())
        } else {
            AppError::from(e)
        }
    })?;

    let library = sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} WHERE id = ?"))
        .bind(result.last_insert_rowid())
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "library": library })))
}

pub async fn update(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<i64>,
    Json(body): Json<UpsertLibraryRequest>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let name = body.name.trim().to_string();
    let path = body.path.trim().trim_end_matches('/').to_string();
    if name.is_empty() || path.is_empty() {
        return Err(AppError::BadRequest("Name and path are required".into()));
    }
    if Library::path_needs_config(&path) {
        return Err(AppError::BadRequest(
            "Set a real container path (the mount point inside this container)".into(),
        ));
    }

    let existing = sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let abs_id = body.abs_id.or(existing.abs_id);

    let result = sqlx::query(
        "UPDATE libraries SET name = ?, path = ?, abs_id = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(&path)
    .bind(abs_id.as_deref())
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Conflict("Library name or path already exists".into())
        } else {
            AppError::from(e)
        }
    })?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    let library = sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "library": library })))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM libraries")
        .fetch_one(&state.pool)
        .await?;
    if count.0 <= 1 {
        return Err(AppError::BadRequest(
            "At least one library must remain".into(),
        ));
    }
    sqlx::query("DELETE FROM libraries WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// Pull library names / ABS IDs from Audiobookshelf.
/// Does **not** overwrite configured container paths — those are set in Settings.
pub async fn sync_from_abs(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<SyncAbsRequest>,
) -> AppResult<Json<Value>> {
    auth.require_root()?;
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    let base = body
        .audiobookshelf_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(settings.audiobookshelf_url.as_str())
        .trim_end_matches('/');
    let token = body
        .audiobookshelf_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(settings.audiobookshelf_token.as_str());

    if base.is_empty() || token.is_empty() {
        return Err(AppError::BadRequest(
            "Audiobookshelf URL and API token are required".into(),
        ));
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/libraries"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Audiobookshelf request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "Audiobookshelf returned {}",
            resp.status()
        )));
    }
    let payload: Value = resp.json().await.map_err(AppError::internal)?;
    let libs = payload
        .get("libraries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut imported = 0usize;
    let mut needs_path = 0usize;

    for lib in libs {
        let abs_id = lib.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let name = lib
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Library")
            .to_string();
        let abs_folder = lib
            .get("folderPath")
            .or_else(|| {
                lib.get("folders")
                    .and_then(|f| f.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|f| f.get("fullPath").or_else(|| f.get("path")))
            })
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .trim_end_matches('/')
            .to_string();
        let abs_folder_opt = if abs_folder.is_empty() {
            None
        } else {
            Some(abs_folder.as_str())
        };

        // Prefer match by abs_id, then by exact name (legacy rows without abs_id).
        let existing: Option<Library> = if !abs_id.is_empty() {
            sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} WHERE abs_id = ?"))
                .bind(abs_id)
                .fetch_optional(&state.pool)
                .await?
        } else {
            None
        };
        let existing = match existing {
            Some(row) => Some(row),
            None => {
                sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} WHERE name = ?"))
                    .bind(&name)
                    .fetch_optional(&state.pool)
                    .await?
            }
        };

        if let Some(row) = existing {
            sqlx::query(
                "UPDATE libraries SET name = ?, abs_id = ?, abs_path = ? WHERE id = ?",
            )
            .bind(&name)
            .bind(if abs_id.is_empty() {
                row.abs_id.as_deref()
            } else {
                Some(abs_id)
            })
            .bind(abs_folder_opt.or(row.abs_path.as_deref()))
            .bind(row.id)
            .execute(&state.pool)
            .await?;
            if Library::path_needs_config(&row.path) {
                needs_path += 1;
            }
            imported += 1;
            continue;
        }

        // New library: placeholder container path until admin assigns a mount.
        let provisional = unset_path_for(abs_id, &name);
        let res = sqlx::query(
            "INSERT INTO libraries (name, path, abs_id, abs_path) VALUES (?, ?, ?, ?)",
        )
        .bind(&name)
        .bind(&provisional)
        .bind(if abs_id.is_empty() {
            None
        } else {
            Some(abs_id)
        })
        .bind(abs_folder_opt)
        .execute(&state.pool)
        .await;
        if res.is_ok() {
            imported += 1;
            needs_path += 1;
        }
    }

    if body.audiobookshelf_url.as_deref().is_some_and(|s| !s.trim().is_empty())
        || body
            .audiobookshelf_token
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    {
        sqlx::query(
            r#"
            UPDATE settings SET
                audiobookshelf_url = CASE WHEN ? != '' THEN ? ELSE audiobookshelf_url END,
                audiobookshelf_token = CASE WHEN ? != '' THEN ? ELSE audiobookshelf_token END,
                updated_at = datetime('now')
            WHERE id = 1
            "#,
        )
        .bind(base)
        .bind(base)
        .bind(token)
        .bind(token)
        .execute(&state.pool)
        .await?;
    }

    let libraries = sqlx::query_as::<_, Library>(&format!("{LIBRARY_SELECT} ORDER BY name"))
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(json!({
        "imported": imported,
        "needs_path": needs_path,
        "libraries": libraries
    })))
}
