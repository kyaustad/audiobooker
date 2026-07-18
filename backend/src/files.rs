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
/// the configured download root. Also tries incomplete→complete sibling folders.
pub fn resolve_download_source(
    content_path: Option<&str>,
    save_path: Option<&str>,
    local_download_root: &str,
) -> PathBuf {
    for candidate in content_path_candidates(content_path) {
        let p = Path::new(&candidate);
        if p.exists() {
            return p.to_path_buf();
        }
    }

    let local_roots = local_root_candidates(local_download_root);

    // Remap: strip qBit save_path prefix and join onto each local root candidate.
    if let (Some(content), Some(save)) = (
        content_path.filter(|s| !s.is_empty()),
        save_path.filter(|s| !s.is_empty()),
    ) {
        for content_alt in content_path_candidates(Some(content)) {
            for save_alt in content_path_candidates(Some(save)) {
                if let Ok(rel) = Path::new(&content_alt).strip_prefix(&save_alt) {
                    for local in &local_roots {
                        let mapped = local.join(rel);
                        if mapped.exists() {
                            return mapped;
                        }
                    }
                    // Prefer first local root even if missing (caller may join further).
                    if let Some(local) = local_roots.first() {
                        return local.join(rel);
                    }
                }
            }
        }
    }

    // Fall back: last path component under each local download root.
    if let Some(content) = content_path.filter(|s| !s.is_empty()) {
        if let Some(name) = Path::new(content).file_name() {
            for local in &local_roots {
                let mapped = local.join(name);
                if mapped.exists() {
                    return mapped;
                }
            }
            if let Some(local) = local_roots.first() {
                return local.join(name);
            }
        }
        return PathBuf::from(content);
    }

    if let Some(save) = save_path.filter(|s| !s.is_empty()) {
        for alt in content_path_candidates(Some(save)) {
            let p = Path::new(&alt);
            if p.exists() {
                return p.to_path_buf();
            }
        }
        if let Some(local) = local_roots.first() {
            return local.clone();
        }
        return PathBuf::from(save);
    }

    local_roots
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Resolve a pack item's on-disk path, trying incomplete→complete and avoiding
/// doubled torrent-root folders when joining relative qBit file names.
pub fn resolve_item_source(
    content_path: Option<&str>,
    save_path: Option<&str>,
    local_download_root: &str,
    relative: &str,
) -> PathBuf {
    let rel = relative.trim_start_matches('/').replace('\\', "/");
    let mut tried = Vec::new();

    let push_try = |path: PathBuf, tried: &mut Vec<PathBuf>| {
        if !tried.iter().any(|p| p == &path) {
            tried.push(path);
        }
    };

    let root = resolve_download_source(content_path, save_path, local_download_root);
    for joined in join_source_variants(&root, &rel) {
        push_try(joined, &mut tried);
    }

    for content_alt in content_path_candidates(content_path) {
        let root_alt = PathBuf::from(&content_alt);
        for joined in join_source_variants(&root_alt, &rel) {
            push_try(joined, &mut tried);
        }
    }

    for local in local_root_candidates(local_download_root) {
        for joined in join_source_variants(&local, &rel) {
            push_try(joined, &mut tried);
        }
        // Bare relative under local root (common when save_path changed category).
        push_try(local.join(&rel), &mut tried);
        if let Some(name) = Path::new(&rel).file_name() {
            push_try(local.join(name), &mut tried);
        }
    }

    for path in &tried {
        if path.exists() {
            return path.clone();
        }
    }

    tried.into_iter().next().unwrap_or_else(|| PathBuf::from(&rel))
}

fn content_path_candidates(path: Option<&str>) -> Vec<String> {
    let Some(path) = path.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let mut out = vec![path.to_string()];
    let replacements = [
        ("/incomplete/", "/complete/"),
        ("/Incomplete/", "/Complete/"),
        ("/incomplete/", "/completed/"),
        ("\\incomplete\\", "\\complete\\"),
    ];
    for (from, to) in replacements {
        if path.contains(from) {
            out.push(path.replace(from, to));
        }
    }
    // Trailing folder name swap: .../incomplete → .../complete
    let p = Path::new(path);
    if let (Some(parent), Some(name)) = (p.parent(), p.file_name().and_then(|s| s.to_str())) {
        let lower = name.to_ascii_lowercase();
        if lower == "incomplete" {
            out.push(parent.join("complete").to_string_lossy().into());
            out.push(parent.join("completed").to_string_lossy().into());
        }
    }
    out
}

fn local_root_candidates(local_download_root: &str) -> Vec<PathBuf> {
    let local_root = local_download_root.trim();
    if local_root.is_empty() {
        return Vec::new();
    }
    let primary = PathBuf::from(local_root);
    let mut roots = vec![primary.clone()];
    if let (Some(parent), Some(name)) = (
        primary.parent(),
        primary.file_name().and_then(|s| s.to_str()),
    ) {
        let lower = name.to_ascii_lowercase();
        if lower.contains("incomplete") {
            // Prefer complete siblings first so finished torrents resolve correctly.
            let mut preferred = Vec::new();
            for alt in ["complete", "completed", "done"] {
                let candidate = parent.join(alt);
                if !roots.contains(&candidate) {
                    preferred.push(candidate);
                }
            }
            preferred.append(&mut roots);
            return preferred;
        } else if lower == "complete" || lower == "completed" {
            let candidate = parent.join("incomplete");
            if !roots.contains(&candidate) {
                roots.push(candidate);
            }
        }
    }
    roots
}

fn join_source_variants(root: &Path, relative: &str) -> Vec<PathBuf> {
    let rel = relative.trim_start_matches('/').replace('\\', "/");
    if rel.is_empty() {
        return vec![root.to_path_buf()];
    }

    let mut out = vec![root.join(&rel)];

    // Avoid doubling torrent root: content_path often ends with the same folder
    // that prefixes qBit file names (e.g. root=/…/Pack, rel=Pack/Book1/…).
    let root_name = root.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if let Some(first) = rel.split('/').next() {
        if !root_name.is_empty() && root_name == first {
            let stripped = rel[first.len()..].trim_start_matches('/');
            if stripped.is_empty() {
                out.push(root.to_path_buf());
            } else {
                out.push(root.join(stripped));
            }
        }
    }

    out
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

    #[test]
    fn strips_doubled_torrent_root() {
        let variants = join_source_variants(
            Path::new("/downloads/My Pack"),
            "My Pack/Book One/chapter.m4b",
        );
        assert!(variants.contains(&PathBuf::from(
            "/downloads/My Pack/Book One/chapter.m4b"
        )));
    }

    #[test]
    fn incomplete_to_complete_candidate() {
        let alts = content_path_candidates(Some("/data/incomplete/My Pack"));
        assert!(alts.iter().any(|p| p.contains("/complete/")));
    }
}

pub async fn copy_completed(
    source: &Path,
    library_root: &Path,
    relative: &Path,
) -> AppResult<PathBuf> {
    copy_sources_into_library(&[source.to_path_buf()], library_root, relative).await
}

/// Copy one or more torrent paths into the same library book folder.
/// Existing destination directory is merged (additional files allowed).
pub async fn copy_sources_into_library(
    sources: &[PathBuf],
    library_root: &Path,
    relative: &Path,
) -> AppResult<PathBuf> {
    if sources.is_empty() {
        return Err(AppError::Internal("No source paths to copy".into()));
    }
    for source in sources {
        if !source.exists() {
            return Err(AppError::Internal(format!(
                "Source path does not exist: {} (check that /downloads matches qBittorrent's completed files and Settings → Download path)",
                source.display()
            )));
        }
    }

    let destination = library_root.join(relative);

    // Single directory source: recursive copy into book folder (merge if exists).
    if sources.len() == 1 {
        let source = &sources[0];
        let meta = fs::metadata(source)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        if meta.is_dir() {
            if destination.exists() {
                merge_dir_recursive(source, &destination).await?;
            } else {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|e| AppError::internal(e.to_string()))?;
                }
                copy_dir_recursive(source, &destination).await?;
            }
            return Ok(destination);
        }
    }

    fs::create_dir_all(&destination)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    for source in sources {
        let meta = fs::metadata(source)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        if meta.is_dir() {
            merge_dir_recursive(source, &destination).await?;
            continue;
        }
        let file_name = source
            .file_name()
            .ok_or_else(|| AppError::Internal("Invalid source file".into()))?;
        let dest_file = destination.join(file_name);
        if dest_file.exists() {
            tracing::info!(
                dest = %dest_file.display(),
                "file already exists in book folder — skipping"
            );
            continue;
        }
        fs::copy(source, &dest_file)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
    }

    Ok(destination)
}

async fn merge_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
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
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            Box::pin(merge_dir_recursive(&from, &to)).await?;
        } else if to.exists() {
            tracing::info!(dest = %to.display(), "file already exists — skipping");
        } else {
            fs::copy(&from, &to)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
}

/// Build a browseable list of relative paths from qBit file names (files only)
/// plus inferred parent directories.
pub fn entries_from_qb_paths(paths: &[(String, i64)]) -> Vec<ContentEntry> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut files: BTreeMap<String, i64> = BTreeMap::new();
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for (name, size) in paths {
        let clean = name.trim_start_matches('/').replace('\\', "/");
        if clean.is_empty() {
            continue;
        }
        files.insert(clean.clone(), *size);
        let mut acc = String::new();
        let parts: Vec<_> = clean.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            if i + 1 == parts.len() {
                break;
            }
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(part);
            dirs.insert(acc.clone());
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        let name = d.rsplit('/').next().unwrap_or(&d).to_string();
        out.push(ContentEntry {
            path: d,
            name,
            is_dir: true,
            size: 0,
        });
    }
    for (path, size) in files {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        out.push(ContentEntry {
            path: path.clone(),
            name,
            is_dir: false,
            size,
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
    });
    out
}

pub async fn entries_from_disk(root: &Path) -> AppResult<Vec<ContentEntry>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_disk(root, root, &mut out, 0).await?;
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
    });
    Ok(out)
}

async fn collect_disk(
    root: &Path,
    current: &Path,
    out: &mut Vec<ContentEntry>,
    depth: usize,
) -> AppResult<()> {
    if depth > 8 {
        return Ok(());
    }
    let mut entries = fs::read_dir(current)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() {
            continue;
        }
        let file_type = entry
            .file_type()
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_dir() {
            out.push(ContentEntry {
                path: rel,
                name,
                is_dir: true,
                size: 0,
            });
            Box::pin(collect_disk(root, &path, out, depth + 1)).await?;
        } else {
            let size = fs::metadata(&path).await.map(|m| m.len() as i64).unwrap_or(0);
            out.push(ContentEntry {
                path: rel,
                name,
                is_dir: false,
                size,
            });
        }
    }
    Ok(())
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
