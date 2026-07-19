//! Discover-as-you-go AudiobookBay cache.
//! Serves fresh page/details from SQLite when possible; only hits ABB on miss/expiry.
//! On ABB failure, returns stale cache when available.

use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::abb::{AbbDetails, AbbSearchPage, AbbSearchResult};
use crate::error::AppResult;

/// Latest browse page 1 refreshes often so new uploads show up.
const TTL_LATEST_PAGE1_SECS: i64 = 20 * 60;
/// Deeper latest pages change less often.
const TTL_LATEST_DEEP_SECS: i64 = 2 * 60 * 60;
const TTL_SEARCH_SECS: i64 = 2 * 60 * 60;
const TTL_CATEGORY_SECS: i64 = 60 * 60;
/// Detail pages (magnet/hash) rarely change.
const TTL_DETAILS_SECS: i64 = 7 * 24 * 60 * 60;

pub fn page_cache_key(mode: &str, query_key: &str, page: u32) -> String {
    format!("{mode}|{query_key}|{page}")
}

pub fn normalize_listing_url(url: &str) -> String {
    let trimmed = url.trim();
    let without_hash = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_hash.split('?').next().unwrap_or(without_hash);
    without_query.trim_end_matches('/').to_string()
}

fn ttl_for(mode: &str, page: u32) -> i64 {
    match mode {
        "latest" if page <= 1 => TTL_LATEST_PAGE1_SECS,
        "latest" => TTL_LATEST_DEEP_SECS,
        "search" => TTL_SEARCH_SECS,
        "category" => TTL_CATEGORY_SECS,
        _ => TTL_SEARCH_SECS,
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PageCacheRow {
    has_more: bool,
    results_json: String,
    mirror: String,
    category_label: Option<String>,
    mode: String,
    query_key: String,
    page: i64,
    #[allow(dead_code)]
    expires_at: String,
}

pub async fn get_fresh_page(
    pool: &SqlitePool,
    mode: &str,
    query_key: &str,
    page: u32,
) -> AppResult<Option<AbbSearchPage>> {
    let key = page_cache_key(mode, query_key, page);
    let now = Utc::now().to_rfc3339();
    let row = sqlx::query_as::<_, PageCacheRow>(
        r#"
        SELECT has_more, results_json, mirror, category_label, mode, query_key, page, expires_at
        FROM abb_page_cache
        WHERE cache_key = ? AND expires_at > ?
        "#,
    )
    .bind(&key)
    .bind(&now)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| row_to_page(r, false)))
}

pub async fn get_stale_page(
    pool: &SqlitePool,
    mode: &str,
    query_key: &str,
    page: u32,
) -> AppResult<Option<AbbSearchPage>> {
    let key = page_cache_key(mode, query_key, page);
    let row = sqlx::query_as::<_, PageCacheRow>(
        r#"
        SELECT has_more, results_json, mirror, category_label, mode, query_key, page, expires_at
        FROM abb_page_cache
        WHERE cache_key = ?
        "#,
    )
    .bind(&key)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| row_to_page(r, true)))
}

fn row_to_page(row: PageCacheRow, stale: bool) -> Option<AbbSearchPage> {
    let results: Vec<AbbSearchResult> = serde_json::from_str(&row.results_json).ok()?;
    let query = if row.mode == "search" && !row.query_key.is_empty() {
        Some(row.query_key.clone())
    } else {
        None
    };
    let category = if row.mode == "category" && !row.query_key.is_empty() {
        Some(row.query_key.clone())
    } else {
        None
    };
    if stale {
        tracing::info!(
            mode = %row.mode,
            page = row.page,
            "serving stale ABB page cache (upstream unavailable)"
        );
    }
    Some(AbbSearchPage {
        results,
        page: row.page as u32,
        has_more: row.has_more,
        mirror: row.mirror,
        mode: row.mode,
        query,
        category,
        category_label: row.category_label,
    })
}

pub async fn put_page(pool: &SqlitePool, page: &AbbSearchPage) -> AppResult<()> {
    let query_key = match page.mode.as_str() {
        "search" => page
            .query
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase(),
        "category" => page.category.clone().unwrap_or_default(),
        _ => String::new(),
    };
    let key = page_cache_key(&page.mode, &query_key, page.page);
    let ttl = ttl_for(&page.mode, page.page);
    let now = Utc::now();
    let fetched_at = now.to_rfc3339();
    let expires_at = (now + Duration::seconds(ttl)).to_rfc3339();
    let results_json = serde_json::to_string(&page.results).unwrap_or_else(|_| "[]".into());

    sqlx::query(
        r#"
        INSERT INTO abb_page_cache (
            cache_key, mode, query_key, page, has_more, results_json, mirror,
            category_label, fetched_at, expires_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(cache_key) DO UPDATE SET
            has_more = excluded.has_more,
            results_json = excluded.results_json,
            mirror = excluded.mirror,
            category_label = excluded.category_label,
            fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at
        "#,
    )
    .bind(&key)
    .bind(&page.mode)
    .bind(&query_key)
    .bind(page.page as i64)
    .bind(page.has_more)
    .bind(&results_json)
    .bind(&page.mirror)
    .bind(&page.category_label)
    .bind(&fetched_at)
    .bind(&expires_at)
    .execute(pool)
    .await?;

    upsert_listings(pool, &page.results).await?;
    Ok(())
}

pub async fn upsert_listings(pool: &SqlitePool, results: &[AbbSearchResult]) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    for r in results {
        let url = normalize_listing_url(&r.url);
        if url.is_empty() {
            continue;
        }
        sqlx::query(
            r#"
            INSERT INTO abb_listings (
                url, title, cover_url, info, author, language, format, bitrate, size, posted, category,
                discovered_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                cover_url = COALESCE(excluded.cover_url, abb_listings.cover_url),
                info = COALESCE(excluded.info, abb_listings.info),
                author = COALESCE(excluded.author, abb_listings.author),
                language = COALESCE(excluded.language, abb_listings.language),
                format = COALESCE(excluded.format, abb_listings.format),
                bitrate = COALESCE(excluded.bitrate, abb_listings.bitrate),
                size = COALESCE(excluded.size, abb_listings.size),
                posted = COALESCE(excluded.posted, abb_listings.posted),
                category = COALESCE(excluded.category, abb_listings.category),
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&url)
        .bind(&r.title)
        .bind(&r.cover_url)
        .bind(&r.info)
        .bind(&r.author)
        .bind(&r.language)
        .bind(&r.format)
        .bind(&r.bitrate)
        .bind(&r.size)
        .bind(&r.posted)
        .bind(&r.category)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Serve search from previously discovered listings when we have a full page.
pub async fn local_search_page(
    pool: &SqlitePool,
    query: &str,
    page: u32,
    page_size: usize,
    mirror: &str,
) -> AppResult<Option<AbbSearchPage>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    let page = page.max(1);
    let offset = ((page - 1) as usize) * page_size;
    let like = format!("%{}%", q.to_ascii_lowercase());

    let total: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM abb_listings
        WHERE lower(title) LIKE ? OR lower(COALESCE(author, '')) LIKE ?
        "#,
    )
    .bind(&like)
    .bind(&like)
    .fetch_one(pool)
    .await?;

    if total.0 == 0 {
        return Ok(None);
    }

    // Only skip ABB when we can fill this page (or page 1 has anything and we're past first page needs).
    let need = if page == 1 {
        1
    } else {
        page_size as i64
    };
    if total.0 < (offset as i64) + need {
        return Ok(None);
    }

    #[derive(sqlx::FromRow)]
    struct ListingRow {
        url: String,
        title: String,
        cover_url: Option<String>,
        info: Option<String>,
        author: Option<String>,
        language: Option<String>,
        format: Option<String>,
        bitrate: Option<String>,
        size: Option<String>,
        posted: Option<String>,
        category: Option<String>,
    }

    let rows = sqlx::query_as::<_, ListingRow>(
        r#"
        SELECT url, title, cover_url, info, author, language, format, bitrate, size, posted, category
        FROM abb_listings
        WHERE lower(title) LIKE ? OR lower(COALESCE(author, '')) LIKE ?
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(&like)
    .bind(&like)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }
    // For pages > 1 require a reasonably full page so we don't under-serve vs ABB.
    if page > 1 && rows.len() < page_size {
        return Ok(None);
    }

    let results: Vec<AbbSearchResult> = rows
        .into_iter()
        .map(|r| AbbSearchResult {
            title: r.title,
            url: r.url,
            cover_url: r.cover_url,
            info: r.info,
            author: r.author,
            language: r.language,
            format: r.format,
            bitrate: r.bitrate,
            size: r.size,
            posted: r.posted,
            category: r.category,
        })
        .collect();

    let has_more = (offset as i64) + (results.len() as i64) < total.0;
    tracing::info!(
        %query,
        page,
        n = results.len(),
        total = total.0,
        "ABB search served from local listing cache"
    );

    Ok(Some(AbbSearchPage {
        results,
        page,
        has_more,
        mirror: mirror.to_string(),
        mode: "search".into(),
        query: Some(q.to_string()),
        category: None,
        category_label: None,
    }))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DetailsRow {
    title: String,
    url: String,
    info_hash: Option<String>,
    magnet_uri: Option<String>,
    cover_url: Option<String>,
    description: Option<String>,
    author: Option<String>,
    narrator: Option<String>,
    format: Option<String>,
    bitrate: Option<String>,
    size: Option<String>,
}

pub async fn get_fresh_details(pool: &SqlitePool, url: &str) -> AppResult<Option<AbbDetails>> {
    let url = normalize_listing_url(url);
    let now = Utc::now().to_rfc3339();
    let row = sqlx::query_as::<_, DetailsRow>(
        r#"
        SELECT title, url, info_hash, magnet_uri, cover_url, description, author, narrator, format, bitrate, size
        FROM abb_details
        WHERE url = ? AND expires_at > ?
        "#,
    )
    .bind(&url)
    .bind(&now)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_details))
}

pub async fn get_stale_details(pool: &SqlitePool, url: &str) -> AppResult<Option<AbbDetails>> {
    let url = normalize_listing_url(url);
    let row = sqlx::query_as::<_, DetailsRow>(
        r#"
        SELECT title, url, info_hash, magnet_uri, cover_url, description, author, narrator, format, bitrate, size
        FROM abb_details
        WHERE url = ?
        "#,
    )
    .bind(&url)
    .fetch_optional(pool)
    .await?;
    if row.is_some() {
        tracing::info!(%url, "serving stale ABB details cache");
    }
    Ok(row.map(row_to_details))
}

fn row_to_details(r: DetailsRow) -> AbbDetails {
    AbbDetails {
        title: r.title,
        url: r.url,
        info_hash: r.info_hash,
        magnet_uri: r.magnet_uri,
        cover_url: r.cover_url,
        description: r.description,
        author: r.author,
        narrator: r.narrator,
        format: r.format,
        bitrate: r.bitrate,
        size: r.size,
    }
}

pub async fn put_details(pool: &SqlitePool, d: &AbbDetails) -> AppResult<()> {
    let url = normalize_listing_url(&d.url);
    let now = Utc::now();
    let fetched_at = now.to_rfc3339();
    let expires_at = (now + Duration::seconds(TTL_DETAILS_SECS)).to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO abb_details (
            url, title, info_hash, magnet_uri, cover_url, description, author, narrator,
            format, bitrate, size, fetched_at, expires_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(url) DO UPDATE SET
            title = excluded.title,
            info_hash = COALESCE(excluded.info_hash, abb_details.info_hash),
            magnet_uri = COALESCE(excluded.magnet_uri, abb_details.magnet_uri),
            cover_url = COALESCE(excluded.cover_url, abb_details.cover_url),
            description = COALESCE(excluded.description, abb_details.description),
            author = COALESCE(excluded.author, abb_details.author),
            narrator = COALESCE(excluded.narrator, abb_details.narrator),
            format = COALESCE(excluded.format, abb_details.format),
            bitrate = COALESCE(excluded.bitrate, abb_details.bitrate),
            size = COALESCE(excluded.size, abb_details.size),
            fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at
        "#,
    )
    .bind(&url)
    .bind(&d.title)
    .bind(&d.info_hash)
    .bind(&d.magnet_uri)
    .bind(&d.cover_url)
    .bind(&d.description)
    .bind(&d.author)
    .bind(&d.narrator)
    .bind(&d.format)
    .bind(&d.bitrate)
    .bind(&d.size)
    .bind(&fetched_at)
    .bind(&expires_at)
    .execute(pool)
    .await?;

    // Keep listing row in sync when we learn more from details.
    let listing = AbbSearchResult {
        title: d.title.clone(),
        url: url.clone(),
        cover_url: d.cover_url.clone(),
        info: None,
        author: d.author.clone(),
        language: None,
        format: d.format.clone(),
        bitrate: d.bitrate.clone(),
        size: d.size.clone(),
        posted: None,
        category: None,
    };
    upsert_listings(pool, &[listing]).await?;
    Ok(())
}

#[allow(dead_code)]
pub fn cache_stats_json(pages: i64, listings: i64, details: i64) -> Value {
    serde_json::json!({ "pages": pages, "listings": listings, "details": details })
}
