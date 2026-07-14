use std::path::{Path, PathBuf};

use tokio::fs;

use crate::error::{AppError, AppResult};
use crate::models::BookMetadataPublic;

fn sanitize_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "Unknown".into()
    } else {
        trimmed
    }
}

pub fn build_library_relative_path(template: &str, meta: &BookMetadataPublic) -> PathBuf {
    let author = meta
        .authors
        .first()
        .map(|s| s.as_str())
        .unwrap_or("Unknown Author");
    let series = meta.series.as_deref().unwrap_or("");
    let title = if let Some(idx) = meta.series_index.as_deref().filter(|s| !s.is_empty()) {
        format!("{idx} - {}", meta.title)
    } else {
        meta.title.clone()
    };

    let mut path = PathBuf::new();
    // Default ABS-like structure if template contains placeholders
    if template.contains("{Author}") || template.contains("{Series}") || template.contains("{Title}")
    {
        path.push(sanitize_component(author));
        if !series.is_empty() {
            path.push(sanitize_component(series));
        }
        path.push(sanitize_component(&title));
        return path;
    }

    path.push(sanitize_component(author));
    if !series.is_empty() {
        path.push(sanitize_component(series));
    }
    path.push(sanitize_component(&title));
    path
}

/// Resolve a qBittorrent content/save path into a path that exists inside this container.
///
/// qBit often reports host paths (e.g. `/mnt/user/downloads/Book`) while Audiobooker
/// mounts the same share at `local_download_root` (e.g. `/downloads`). Prefer the path
/// as-is when it exists; otherwise remap by joining the trailing relative segment onto
/// the configured download root.
pub fn resolve_download_source(
    content_path: Option<&str>,
    save_path: Option<&str>,
    local_download_root: &str,
) -> PathBuf {
    // Prefer the exact content path when it exists inside this container.
    if let Some(content) = content_path.filter(|s| !s.is_empty()) {
        let p = Path::new(content);
        if p.exists() {
            return p.to_path_buf();
        }
    }

    let local_root = local_download_root.trim();
    let local = if local_root.is_empty() {
        None
    } else {
        Some(Path::new(local_root))
    };

    // Remap: strip qBit save_path prefix and join onto the configured download root.
    if let (Some(content), Some(save), Some(local)) = (
        content_path.filter(|s| !s.is_empty()),
        save_path.filter(|s| !s.is_empty()),
        local,
    ) {
        if let Ok(rel) = Path::new(content).strip_prefix(save) {
            return local.join(rel);
        }
    }

    // Fall back: last path component under the local download root.
    if let (Some(content), Some(local)) = (content_path.filter(|s| !s.is_empty()), local) {
        if let Some(name) = Path::new(content).file_name() {
            return local.join(name);
        }
        return PathBuf::from(content);
    }

    if let Some(content) = content_path.filter(|s| !s.is_empty()) {
        return PathBuf::from(content);
    }

    if let Some(save) = save_path.filter(|s| !s.is_empty()) {
        let p = Path::new(save);
        if p.exists() {
            return p.to_path_buf();
        }
        if let Some(local) = local {
            return local.to_path_buf();
        }
        return PathBuf::from(save);
    }

    local
        .map(|p| p.to_path_buf())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaps_relative_to_local_root() {
        let mapped = resolve_download_source(
            Some("/mnt/user/downloads/Some Book"),
            Some("/mnt/user/downloads"),
            "/downloads",
        );
        assert_eq!(mapped, PathBuf::from("/downloads/Some Book"));
    }
}

pub async fn copy_completed(
    source: &Path,
    library_root: &Path,
    relative: &Path,
) -> AppResult<PathBuf> {
    if !source.exists() {
        return Err(AppError::Internal(format!(
            "Source path does not exist: {} (check that /downloads matches qBittorrent's completed files and Settings → Download path)",
            source.display()
        )));
    }

    let destination = library_root.join(relative);
    if destination.exists() {
        return Err(AppError::Conflict(format!(
            "Destination already exists: {}",
            destination.display()
        )));
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
    }

    let meta = fs::metadata(source)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    if meta.is_dir() {
        copy_dir_recursive(source, &destination).await?;
    } else {
        fs::create_dir_all(&destination)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        let file_name = source
            .file_name()
            .ok_or_else(|| AppError::Internal("Invalid source file".into()))?;
        fs::copy(source, destination.join(file_name))
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
    }

    Ok(destination)
}

async fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    fs::create_dir_all(dst)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let mut entries = fs::read_dir(src)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            Box::pin(copy_dir_recursive(&entry.path(), &target)).await?;
        } else {
            fs::copy(entry.path(), target)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }
    }
    Ok(())
}
