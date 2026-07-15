use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::magnet::normalize_info_hash;

/// Only include mirrors that currently serve usable HTML/RSS over HTTPS.
const ABB_MIRRORS: &[&str] = &["https://audiobookbay.lu"];

/// WordPress lists ~9–10 posts per page on ABB.
const ABB_PAGE_SIZE: usize = 9;

#[derive(Debug, Clone, Serialize)]
pub struct AbbSearchResult {
    pub title: String,
    pub url: String,
    pub cover_url: Option<String>,
    pub info: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub format: Option<String>,
    pub bitrate: Option<String>,
    pub size: Option<String>,
    pub posted: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbbSearchPage {
    pub results: Vec<AbbSearchResult>,
    pub page: u32,
    pub has_more: bool,
    pub mirror: String,
    /// `latest` | `search` | `category`
    pub mode: String,
    pub query: Option<String>,
    pub category: Option<String>,
    pub category_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbbCategory {
    pub slug: String,
    pub label: String,
    pub group: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbbDetails {
    pub title: String,
    pub url: String,
    pub info_hash: Option<String>,
    pub magnet_uri: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub narrator: Option<String>,
    pub format: Option<String>,
    pub bitrate: Option<String>,
    pub size: Option<String>,
}

#[derive(Clone)]
pub struct AbbClient {
    http: Client,
}

impl Default for AbbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AbbClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(10))
                .cookie_store(true)
                .gzip(true)
                .user_agent(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
                )
                .build()
                .expect("reqwest client"),
        }
    }

    /// Homepage / “recent uploads” feed (Discover default).
    pub async fn latest(&self, page: u32) -> AppResult<AbbSearchPage> {
        let page = page.max(1);
        let base = ABB_MIRRORS[0];

        let url = if page <= 1 {
            format!("{base}/feed/")
        } else {
            format!("{base}/feed/?paged={page}")
        };

        tracing::info!(%url, "ABB latest RSS fetch");
        let xml = self.fetch_text(&url, base, "application/rss+xml, application/xml, text/xml, */*;q=0.8").await?;
        let results = parse_rss_feed(&xml, base);
        if results.is_empty() && page == 1 {
            // HTML homepage fallback
            let html = self.fetch_text(&format!("{base}/"), base, "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8").await?;
            let results = parse_listing(&html, base);
            if results.is_empty() {
                return Err(AppError::Internal(
                    "Could not load AudiobookBay latest feed".into(),
                ));
            }
            return Ok(AbbSearchPage {
                has_more: results.len() >= ABB_PAGE_SIZE,
                results,
                page,
                mirror: base.to_string(),
                mode: "latest".into(),
                query: None,
                category: None,
                category_label: None,
            });
        }

        Ok(AbbSearchPage {
            has_more: results.len() >= ABB_PAGE_SIZE,
            results,
            page,
            mirror: base.to_string(),
            mode: "latest".into(),
            query: None,
            category: None,
            category_label: None,
        })
    }

    /// Browse an ABB type/category page, e.g. `/audio-books/type/bestsellers/`.
    pub async fn category(&self, slug: &str, page: u32) -> AppResult<AbbSearchPage> {
        let slug = normalize_category_slug(slug).ok_or_else(|| {
            AppError::BadRequest("Unknown or invalid AudiobookBay category".into())
        })?;
        let label = category_label_for(&slug);
        let page = page.max(1);
        let base = ABB_MIRRORS[0];

        let url = if page <= 1 {
            format!("{base}/audio-books/type/{slug}/")
        } else {
            format!("{base}/audio-books/type/{slug}/page/{page}/")
        };

        tracing::info!(%url, "ABB category fetch");
        let html = self
            .fetch_text(&url, base, "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
            .await?;
        if !is_abb_search_results_page(&html) {
            return Err(AppError::Internal(format!(
                "AudiobookBay category '{slug}' did not return a listing page"
            )));
        }
        let results = parse_listing(&html, base);
        Ok(AbbSearchPage {
            has_more: detect_has_more(&html, page) && !results.is_empty(),
            results,
            page,
            mirror: base.to_string(),
            mode: "category".into(),
            query: None,
            category: Some(slug),
            category_label: Some(label),
        })
    }

    pub fn categories() -> Vec<AbbCategory> {
        abb_categories()
    }

    pub async fn search(&self, query: &str, page: u32) -> AppResult<AbbSearchPage> {
        let q = query.trim();
        if q.is_empty() {
            return Err(AppError::BadRequest("Search query required".into()));
        }
        let page = page.max(1);
        // ABB 301s mixed-case `?s=` to the homepage (losing the query). Always
        // use lowercase like https://audiobookbay.lu/?s=sunrise+on+the+reaping&cat=undefined%2Cundefined
        let encoded = encode_abb_query(q);
        let base = ABB_MIRRORS[0];

        // Prefer the same HTML URL the browser search form effectively uses, then RSS.
        let html_candidates: Vec<String> = if page <= 1 {
            vec![
                format!("{base}/?s={encoded}&cat=undefined%2Cundefined"),
                format!("{base}/?s={encoded}"),
            ]
        } else {
            vec![
                format!("{base}/page/{page}/?s={encoded}&cat=undefined%2Cundefined"),
                format!("{base}/page/{page}/?s={encoded}"),
            ]
        };

        for url in &html_candidates {
            tracing::info!(%url, "ABB search HTML fetch");
            match self
                .fetch_text(url, base, "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
                .await
            {
                Ok(html) => {
                    if is_abb_search_results_page(&html) {
                        let results = parse_listing(&html, base);
                        return Ok(AbbSearchPage {
                            has_more: detect_has_more(&html, page) && !results.is_empty(),
                            results,
                            page,
                            mirror: base.to_string(),
                            mode: "search".into(),
                            query: Some(q.to_string()),
                            category: None,
                            category_label: None,
                        });
                    }
                    tracing::warn!(%url, "ABB HTML was not a search results page");
                }
                Err(err) => {
                    tracing::warn!(error = %err, %url, "ABB search HTML failed");
                }
            }
        }

        let rss_url = if page <= 1 {
            format!("{base}/?s={encoded}&feed=rss2")
        } else {
            format!("{base}/?s={encoded}&feed=rss2&paged={page}")
        };

        tracing::info!(%rss_url, "ABB search RSS fetch");
        match self
            .fetch_text(
                &rss_url,
                base,
                "application/rss+xml, application/xml, text/xml, */*;q=0.8",
            )
            .await
        {
            Ok(xml) => {
                if rss_is_search_feed(&xml, &encoded) {
                    let results = parse_rss_feed(&xml, base);
                    return Ok(AbbSearchPage {
                        has_more: results.len() >= ABB_PAGE_SIZE,
                        results,
                        page,
                        mirror: base.to_string(),
                        mode: "search".into(),
                        query: Some(q.to_string()),
                        category: None,
                        category_label: None,
                    });
                }
                tracing::warn!(%rss_url, "ABB RSS was not a search feed");
            }
            Err(err) => {
                tracing::warn!(error = %err, %rss_url, "ABB search RSS failed");
            }
        }

        Err(AppError::Internal(
            "AudiobookBay returned the homepage instead of search results. Try again shortly."
                .into(),
        ))
    }

    pub async fn details(&self, path_or_url: &str) -> AppResult<AbbDetails> {
        let url = if path_or_url.starts_with("http") {
            path_or_url.to_string()
        } else if path_or_url.starts_with('/') {
            format!("{}{path_or_url}", ABB_MIRRORS[0])
        } else {
            format!("{}/{path_or_url}", ABB_MIRRORS[0])
        };

        let base = origin_of(&url).unwrap_or_else(|| ABB_MIRRORS[0].to_string());
        let html = self
            .fetch_text(&url, &base, "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
            .await?;
        parse_details(&html, &url, &base).ok_or_else(|| {
            AppError::Internal(
                "Could not parse AudiobookBay page (site layout may have changed)".into(),
            )
        })
    }

    async fn fetch_text(&self, url: &str, origin: &str, accept: &str) -> AppResult<String> {
        let resp = self
            .http
            .get(url)
            .header("Accept", accept)
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", format!("{origin}/"))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("AudiobookBay request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "AudiobookBay returned {} for {url}",
                resp.status()
            )));
        }
        Ok(resp.text().await?)
    }
}

fn encode_abb_query(q: &str) -> String {
    // ABB's nginx/app 301s mixed-case `?s=Sunrise+…` to `/` (homepage). The working
    // browser URL always uses lowercase: ?s=sunrise+on+the+reaping&cat=undefined%2Cundefined
    let q = q.trim().to_ascii_lowercase();
    urlencoding::encode(&q)
        .replace("%20", "+")
        .replace("%2B", "+")
}

fn origin_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    Some(format!("{}://{}", parsed.scheme(), parsed.host_str()?))
}

fn detect_has_more(html: &str, current_page: u32) -> bool {
    let next = current_page + 1;
    if let Ok(re) = regex::Regex::new(r#"/page/(\d+)/"#) {
        let mut max_page = current_page;
        for caps in re.captures_iter(html) {
            if let Ok(n) = caps[1].parse::<u32>() {
                max_page = max_page.max(n);
            }
        }
        if max_page > current_page {
            return true;
        }
    }
    html.contains(&format!("/page/{next}/"))
}

fn is_abb_search_results_page(html: &str) -> bool {
    // Search/archive pages include this heading; the homepage does not.
    html.contains("class=\"archiveTitle\"") || html.contains("class='archiveTitle'")
}

fn abb_categories() -> Vec<AbbCategory> {
    const RAW: &[(&str, &str, &str)] = &[
        // Age
        ("children", "Children", "Age"),
        ("teen-young-adult", "Teen & Young Adult", "Age"),
        ("new", "New", "Age"),
        ("adults", "Adults", "Age"),
        // Category modifiers (like Bestsellers)
        ("bestsellers", "Bestsellers", "Modifiers"),
        ("anthology", "Anthology", "Modifiers"),
        ("classic", "Classic", "Modifiers"),
        ("documentary", "Documentary", "Modifiers"),
        ("full-cast", "Full Cast", "Modifiers"),
        ("libertarian", "Libertarian", "Modifiers"),
        ("military", "Military", "Modifiers"),
        ("novel", "Novel", "Modifiers"),
        ("short-story", "Short Story", "Modifiers"),
        // Genres
        ("postapocalyptic", "(Post)apocalyptic", "Category"),
        ("action", "Action", "Category"),
        ("adventure", "Adventure", "Category"),
        ("art", "Art", "Category"),
        ("autobiography-biographies", "Autobiography & Biographies", "Category"),
        ("business", "Business", "Category"),
        ("computer", "Computer", "Category"),
        ("contemporary", "Contemporary", "Category"),
        ("crime", "Crime", "Category"),
        ("detective", "Detective", "Category"),
        ("doctor-who-sci-fi", "Doctor Who", "Category"),
        ("education", "Education", "Category"),
        ("fantasy", "Fantasy", "Category"),
        ("general-fiction", "General Fiction", "Category"),
        ("general-non-fiction", "Misc. Non-fiction", "Category"),
        ("historical-fiction", "Historical Fiction", "Category"),
        ("history", "History", "Category"),
        ("horror", "Horror", "Category"),
        ("humor", "Humor", "Category"),
        ("lecture", "Lecture", "Category"),
        ("lgbt", "LGBT", "Category"),
        ("light-novel", "Light Novel", "Category"),
        ("literature", "Literature", "Category"),
        ("litrpg", "LitRPG", "Category"),
        ("mystery", "Mystery", "Category"),
        ("paranormal", "Paranormal", "Category"),
        ("plays-theater", "Plays & Theater", "Category"),
        ("poetry", "Poetry", "Category"),
        ("political", "Political", "Category"),
        ("radio-productions", "Radio Productions", "Category"),
        ("romance", "Romance", "Category"),
        ("sci-fi", "Sci-Fi", "Category"),
        ("science", "Science", "Category"),
        ("self-help", "Self-help", "Category"),
        ("spiritual", "Spiritual & Religious", "Category"),
        ("sports", "Sport & Recreation", "Category"),
        ("suspense", "Suspense", "Category"),
        ("thriller", "Thriller", "Category"),
        ("true-crime", "True Crime", "Category"),
        ("tutorial", "Tutorial", "Category"),
        ("westerns", "Westerns", "Category"),
        ("zombies", "Zombies", "Category"),
        ("other", "Other", "Category"),
    ];
    RAW.iter()
        .map(|(slug, label, group)| AbbCategory {
            slug: (*slug).into(),
            label: (*label).into(),
            group: (*group).into(),
        })
        .collect()
}

fn normalize_category_slug(raw: &str) -> Option<String> {
    let slug = raw.trim().trim_matches('/').to_ascii_lowercase();
    if slug.is_empty() || slug.contains('/') || slug.contains("..") {
        return None;
    }
    abb_categories()
        .into_iter()
        .find(|c| c.slug == slug)
        .map(|c| c.slug)
}

fn category_label_for(slug: &str) -> String {
    abb_categories()
        .into_iter()
        .find(|c| c.slug == slug)
        .map(|c| c.label)
        .unwrap_or_else(|| slug.to_string())
}

/// True when the RSS document is clearly a search feed for this query.
fn rss_is_search_feed(xml: &str, encoded_query: &str) -> bool {
    let lower = xml.to_ascii_lowercase();
    // Homepage feed self-link is /feed — never treat that as search.
    if lower.contains("href=\"http://audiobookbay.lu/feed\"")
        || lower.contains("href=\"https://audiobookbay.lu/feed\"")
        || lower.contains("href=\"http://audiobookbay.lu/feed/\"")
        || lower.contains("href=\"https://audiobookbay.lu/feed/\"")
    {
        return false;
    }
    let needle = format!("s={}", encoded_query.to_ascii_lowercase());
    lower.contains(&needle) && (lower.contains("feed=rss2") || lower.contains("feed=rss"))
}

fn sanitize_xml(xml: &str) -> String {
    // ABB/WordPress occasionally emits bare `&` inside descriptions.
    regex::Regex::new(r"&(?![#a-zA-Z0-9]+;)")
        .map(|re| re.replace_all(xml, "&amp;").into_owned())
        .unwrap_or_else(|_| xml.to_string())
}

fn parse_rss_feed(xml: &str, base: &str) -> Vec<AbbSearchResult> {
    let xml = sanitize_xml(xml);
    let item_re = match regex::Regex::new(r"(?is)<item>(.*?)</item>") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let title_re = regex::Regex::new(r"(?is)<title>(.*?)</title>").ok();
    let link_re = regex::Regex::new(r"(?is)<link>(.*?)</link>").ok();
    let thumb_re =
        regex::Regex::new(r#"(?is)<media:thumbnail[^>]+url=["']([^"']+)["']"#).ok();
    let img_re = regex::Regex::new(r#"(?is)<img[^>]+src=["']([^"']+)["']"#).ok();
    let desc_re = regex::Regex::new(r"(?is)<description>(.*?)</description>").ok();
    let pub_re = regex::Regex::new(r"(?is)<pubDate>(.*?)</pubDate>").ok();
    let cat_re = regex::Regex::new(r"(?is)<category[^>]*>(.*?)</category>").ok();

    let mut results = Vec::new();
    for caps in item_re.captures_iter(&xml) {
        let item = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let raw_title = title_re
            .as_ref()
            .and_then(|re| re.captures(item))
            .and_then(|c| c.get(1))
            .map(|m| strip_cdata(m.as_str()))
            .map(|s| clean_text(&s))
            .unwrap_or_default();
        let href = link_re
            .as_ref()
            .and_then(|re| re.captures(item))
            .and_then(|c| c.get(1))
            .map(|m| clean_text(m.as_str()))
            .unwrap_or_default();
        if raw_title.is_empty() || href.is_empty() {
            continue;
        }
        if href.contains("/feed") || href.contains("/member/") {
            continue;
        }

        let desc = desc_re
            .as_ref()
            .and_then(|re| re.captures(item))
            .and_then(|c| c.get(1))
            .map(|m| html_entity_decode(&strip_cdata(m.as_str())))
            .unwrap_or_default();

        let cover_url = thumb_re
            .as_ref()
            .and_then(|re| re.captures(item).or_else(|| re.captures(&desc)))
            .and_then(|c| c.get(1))
            .map(|m| absolutize(m.as_str(), base))
            .or_else(|| {
                img_re
                    .as_ref()
                    .and_then(|re| re.captures(&desc))
                    .and_then(|c| c.get(1))
                    .map(|m| absolutize(m.as_str(), base))
            });

        let format = capture_labeled(&desc, r"(?is)Format:\s*([A-Za-z0-9]+)");
        let bitrate = capture_labeled(&desc, r"(?is)Bitrate:\s*([^<\n]+)");
        let author = capture_labeled(&desc, r"(?is)Written by\s+([^<\n]+)");
        let (parsed_title, author_from_title) = split_title_author(&raw_title);
        let author = author.or(author_from_title);

        let posted = pub_re
            .as_ref()
            .and_then(|re| re.captures(item))
            .and_then(|c| c.get(1))
            .map(|m| clean_text(m.as_str()))
            .and_then(short_rfc2822);

        let language = cat_re.as_ref().and_then(|re| {
            re.captures_iter(item)
                .map(|c| strip_cdata(c.get(1).map(|m| m.as_str()).unwrap_or("")))
                .map(|s| clean_text(&s))
                .find(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "english"
                            | "german"
                            | "french"
                            | "spanish"
                            | "italian"
                            | "russian"
                            | "chinese"
                            | "japanese"
                            | "portuguese"
                            | "dutch"
                    )
                })
        });

        let category = cat_re.as_ref().and_then(|re| {
            re.captures_iter(item)
                .map(|c| strip_cdata(c.get(1).map(|m| m.as_str()).unwrap_or("")))
                .map(|s| clean_text(&s))
                .find(|s| {
                    let l = s.to_ascii_lowercase();
                    !matches!(
                        l.as_str(),
                        "english"
                            | "german"
                            | "french"
                            | "spanish"
                            | "italian"
                            | "russian"
                            | "chinese"
                            | "japanese"
                            | "portuguese"
                            | "dutch"
                    ) && !l.is_empty()
                })
        });

        let mut meta_bits = Vec::new();
        if let Some(f) = format.as_ref() {
            meta_bits.push(f.clone());
        }
        if let Some(b) = bitrate.as_ref() {
            meta_bits.push(clean_text(b));
        }
        if let Some(p) = posted.as_ref() {
            meta_bits.push(format!("Posted {p}"));
        }
        let info = if meta_bits.is_empty() {
            None
        } else {
            Some(meta_bits.join(" · "))
        };

        results.push(AbbSearchResult {
            title: parsed_title,
            url: absolutize(&href, base),
            cover_url,
            info,
            author,
            language,
            format: format.map(|s| clean_text(&s)),
            bitrate: bitrate.map(|s| clean_text(&s)),
            size: None,
            posted,
            category,
        });
    }
    results
}

fn strip_cdata(s: &str) -> String {
    let t = s.trim();
    if let Some(inner) = t
        .strip_prefix("<![CDATA[")
        .and_then(|x| x.strip_suffix("]]>"))
    {
        inner.to_string()
    } else {
        t.to_string()
    }
}

fn short_rfc2822(s: String) -> Option<String> {
    // "Tue, 18 Mar 2025 08:23:10 +0000" → "18 Mar 2025"
    let parts: Vec<_> = s.split_whitespace().collect();
    if parts.len() >= 4 {
        Some(format!("{} {} {}", parts[1], parts[2], parts[3]))
    } else if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn parse_listing(html: &str, base: &str) -> Vec<AbbSearchResult> {
    let document = Html::parse_document(html);
    // Only the main column — never sidebar "Recent Audiobook" links.
    let content_sel = match Selector::parse("#content") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let Some(content) = document.select(&content_sel).next() else {
        return Vec::new();
    };

    let post_sel = match Selector::parse("div.post") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let title_sel = Selector::parse("div.postTitle h2 a, .postTitle h2 a").unwrap();
    let img_sel = Selector::parse("div.postContent img").unwrap();
    let info_sel = Selector::parse("div.postInfo, .postInfo").unwrap();

    let mut results = Vec::new();
    for post in content.select(&post_sel) {
        let Some(link) = post.select(&title_sel).next() else {
            continue;
        };
        let title = clean_text(&link.text().collect::<String>());
        let href = link.value().attr("href").unwrap_or("").to_string();
        if title.is_empty() || href.is_empty() {
            continue;
        }
        if href.contains("/feed") || href.contains("/member/") || href.contains("/forum/") {
            continue;
        }

        let post_html = post.html();
        let cover_url = post
            .select(&img_sel)
            .next()
            .and_then(|img| {
                img.value()
                    .attr("src")
                    .or_else(|| img.value().attr("data-src"))
            })
            .map(|s| absolutize(s, base));

        let info_blob = post
            .select(&info_sel)
            .next()
            .map(|n| clean_text(&n.text().collect::<String>()))
            .filter(|s| !s.is_empty());

        let language = capture_labeled(&post_html, r"(?is)Language:\s*([^<\n]+)");
        let category = capture_labeled(&post_html, r"(?is)Category:\s*([^<\n]+)");
        let format = capture_labeled(&post_html, r"(?is)Format:\s*(?:<[^>]+>)*\s*([^<\s/]+)");
        let bitrate = capture_labeled(&post_html, r"(?is)Bitrate:\s*(?:<[^>]+>)*\s*([^<\n]+)");
        let size = capture_file_size(&post_html);
        let posted = capture_labeled(&post_html, r"(?is)Posted:\s*([^<\n]+)");
        let (parsed_title, author) = split_title_author(&title);

        let mut meta_bits = Vec::new();
        if let Some(f) = format.as_ref() {
            meta_bits.push(f.clone());
        }
        if let Some(b) = bitrate.as_ref() {
            meta_bits.push(b.clone());
        }
        if let Some(s) = size.as_ref() {
            meta_bits.push(s.clone());
        }
        if let Some(p) = posted.as_ref() {
            meta_bits.push(format!("Posted {p}"));
        }
        let info = if meta_bits.is_empty() {
            info_blob
        } else {
            Some(meta_bits.join(" · "))
        };

        results.push(AbbSearchResult {
            title: parsed_title,
            url: absolutize(&href, base),
            cover_url,
            info,
            author,
            language: language.map(|s| clean_text(&s)),
            format: format.map(|s| clean_text(&s)),
            bitrate: bitrate.map(|s| clean_text(&s)),
            size: size.map(|s| clean_text(&s)),
            posted: posted.map(|s| clean_text(&s)),
            category: category.map(|s| clean_text(&s)),
        });
    }
    results
}

fn parse_details(html: &str, page_url: &str, base: &str) -> Option<AbbDetails> {
    let document = Html::parse_document(html);
    let title_sel = Selector::parse("div.postTitle h1, .postTitle h1, h1").ok()?;
    let raw_title = document
        .select(&title_sel)
        .next()
        .map(|n| clean_text(&n.text().collect::<String>()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "AudiobookBay title".into());
    let (title, author_from_title) = split_title_author(&raw_title);

    let body_text = document.root_element().text().collect::<String>();
    let info_hash = extract_info_hash(html).or_else(|| extract_info_hash(&body_text));

    let magnet_sel = Selector::parse(r#"a[href^="magnet:"]"#).ok()?;
    let magnet_uri = document
        .select(&magnet_sel)
        .next()
        .and_then(|a| a.value().attr("href"))
        .map(|s| s.to_string())
        .or_else(|| {
            info_hash
                .as_ref()
                .map(|h| format!("magnet:?xt=urn:btih:{h}"))
        });

    let img_sel = Selector::parse("div.postContent img, .postContent img, .post img").ok()?;
    let cover_url = document
        .select(&img_sel)
        .next()
        .and_then(|img| {
            img.value()
                .attr("src")
                .or_else(|| img.value().attr("data-src"))
        })
        .map(|s| absolutize(s, base));

    let author = capture_labeled(html, r#"(?is)itemprop="author"[^>]*>\s*([^<]+)"#)
        .or(author_from_title)
        .or_else(|| capture_labeled(html, r"(?is)Written by\s+([^<\n]+)"));
    let narrator = capture_labeled(html, r"(?is)Read by\s+([^<\n]+)");
    let format = capture_labeled(html, r#"(?is)class=['"]format['"][^>]*>\s*([^<]+)"#)
        .or_else(|| capture_labeled(html, r"(?is)Format:\s*([^<\n]+)"));
    let bitrate = capture_labeled(html, r#"(?is)class=['"]bitrate['"][^>]*>\s*([^<]+)"#)
        .or_else(|| capture_labeled(html, r"(?is)Bitrate:\s*([^<\n]+)"));
    let size = capture_labeled(
        html,
        r"(?is)(?:Combined )?File Size:</t[dh]>\s*<t[dh][^>]*>\s*([^<]+)",
    )
    .or_else(|| capture_labeled(html, r"(?is)File Size:\s*([^<\n]+)"));

    let desc_sel = Selector::parse("div.postContent").ok()?;
    let description = document
        .select(&desc_sel)
        .next()
        .map(|n| clean_text(&n.text().collect::<String>()))
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.len() > 1200 {
                format!("{}…", &s[..1197])
            } else {
                s
            }
        });

    Some(AbbDetails {
        title,
        url: page_url.to_string(),
        info_hash,
        magnet_uri,
        cover_url,
        description,
        author: author.map(|s| clean_text(&s)),
        narrator: narrator.map(|s| clean_text(&s)),
        format: format.map(|s| clean_text(&s)),
        bitrate: bitrate.map(|s| clean_text(&s)),
        size: size.map(|s| clean_text(&s)),
    })
}

fn split_title_author(raw: &str) -> (String, Option<String>) {
    let cleaned = clean_text(raw);
    // Common ABB pattern: "Book Title - Author Name"
    if let Some((left, right)) = cleaned.rsplit_once(" - ") {
        let author = right.trim();
        if !author.is_empty()
            && author.len() < 80
            && !author.contains("Collection")
            && author.chars().any(|c| c.is_alphabetic())
        {
            return (left.trim().to_string(), Some(author.to_string()));
        }
    }
    (cleaned, None)
}

fn capture_file_size(hay: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"(?is)File Size:\s*(?:<[^>]+>)?\s*([0-9.]+)\s*(?:</[^>]+>)?\s*([A-Za-z]+)",
    )
    .ok()?;
    let caps = re.captures(hay)?;
    let num = caps.get(1)?.as_str().trim();
    let unit = caps.get(2)?.as_str().trim();
    Some(format!("{num} {unit}"))
}

fn capture_labeled(hay: &str, pattern: &str) -> Option<String> {
    let re = regex::Regex::new(pattern).ok()?;
    let caps = re.captures(hay)?;
    let value = clean_text(caps.get(1)?.as_str());
    if value.is_empty() {
        None
    } else {
        Some(html_entity_decode(&value))
    }
}

fn clean_text(s: &str) -> String {
    html_entity_decode(
        &s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace('\u{00a0}', " "),
    )
    .trim()
    .to_string()
}

fn html_entity_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#8217;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn extract_info_hash(text: &str) -> Option<String> {
    if let Ok(re) = regex::Regex::new(
        r"(?is)info\s*hash\s*:?\s*</t[dh]>\s*<t[dh][^>]*>\s*([a-f0-9]{40})\s*</t[dh]>",
    ) {
        if let Some(caps) = re.captures(text) {
            return Some(caps[1].to_ascii_lowercase());
        }
    }

    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("info hash") || lower.contains("infohash") {
            for token in line.split(|c: char| !c.is_ascii_hexdigit()) {
                if let Some(hash) = normalize_info_hash(token) {
                    return Some(hash);
                }
            }
        }
    }

    let re = regex::Regex::new(r"(?i)\b([a-f0-9]{40})\b").ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_ascii_lowercase())
}

fn absolutize(url: &str, base: &str) -> String {
    if url.starts_with("http") {
        url.to_string()
    } else if url.starts_with("//") {
        format!("https:{url}")
    } else if url.starts_with('/') {
        format!("{base}{url}")
    } else {
        format!("{base}/{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_card_fields() {
        let html = r#"
        <div id="content">
          <h1 class="archiveTitle"><strong>Sunrise</strong></h1>
          <div class="post">
            <div class="postTitle"><h2><a href="/abss/example/">Sunrise on the Reaping: A Hunger Games Novel - Suzanne Collins</a></h2></div>
            <div class="postInfo">Category: Fantasy<br />Language: English</div>
            <div class="postContent">
              <img src="https://example.com/cover.jpg" />
              <p>Posted: 18 Mar 2025<br />Format: <span>M4B</span> / Bitrate: <span>128 Kbps</span><br />File Size: <span>698.91</span> MBs</p>
            </div>
          </div>
        </div>
        "#;
        let results = parse_listing(html, "https://audiobookbay.lu");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].title,
            "Sunrise on the Reaping: A Hunger Games Novel"
        );
        assert_eq!(results[0].author.as_deref(), Some("Suzanne Collins"));
        assert_eq!(results[0].format.as_deref(), Some("M4B"));
        assert!(results[0].size.as_deref().unwrap_or("").contains("698.91"));
    }

    #[test]
    fn encodes_like_wordpress() {
        assert_eq!(
            encode_abb_query("sunrise on the reaping"),
            "sunrise+on+the+reaping"
        );
        // Mixed case must lower — ABB redirects capitalized s= to homepage.
        assert_eq!(
            encode_abb_query("Sunrise on the Reaping"),
            "sunrise+on+the+reaping"
        );
        assert_eq!(
            encode_abb_query("Sunrise on the Reaping:"),
            "sunrise+on+the+reaping%3A"
        );
    }

    #[test]
    fn parses_rss_search_items() {
        let xml = r#"<?xml version="1.0"?>
        <rss><channel>
          <atom:link href="http://audiobookbay.lu/?s=sunrise+on+the+reaping&amp;feed=rss2" rel="self"/>
          <item>
            <title>Sunrise on the Reaping: A Hunger Games Novel - Suzanne Collins</title>
            <link>https://audiobookbay.lu/abss/example/</link>
            <description><![CDATA[Written by Suzanne Collins Format: M4B Bitrate: 128 Kbps<br /><img src="https://example.com/c.jpg" />]]></description>
            <media:thumbnail url="https://example.com/c.jpg"/>
            <pubDate>Tue, 18 Mar 2025 08:23:10 +0000</pubDate>
            <category><![CDATA[English]]></category>
            <category><![CDATA[Fantasy]]></category>
          </item>
        </channel></rss>"#;
        assert!(rss_is_search_feed(xml, "sunrise+on+the+reaping"));
        let results = parse_rss_feed(xml, "https://audiobookbay.lu");
        assert_eq!(results.len(), 1);
        assert!(results[0].title.contains("Sunrise"));
        assert_eq!(results[0].format.as_deref(), Some("M4B"));
        assert_eq!(
            results[0].cover_url.as_deref(),
            Some("https://example.com/c.jpg")
        );
    }

    #[test]
    fn rejects_homepage_rss_as_search() {
        let xml = r#"<?xml version="1.0"?><rss><channel>
          <atom:link href="http://audiobookbay.lu/feed" rel="self"/>
          <item><title>Final Strike</title><link>https://audiobookbay.lu/abss/x/</link></item>
        </channel></rss>"#;
        assert!(!rss_is_search_feed(xml, "sunrise+on+the+reaping"));
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_search_matches_abb() {
        let client = AbbClient::new();
        // Title Case is what users type — must not 301 to homepage.
        let page = client
            .search("Sunrise on the Reaping", 1)
            .await
            .expect("search");
        let titles: Vec<_> = page.results.iter().map(|r| r.title.clone()).collect();
        eprintln!("mirror={} n={} titles={:?}", page.mirror, titles.len(), titles);
        assert_eq!(page.mode, "search");
        assert!(
            titles
                .iter()
                .any(|t| t.to_lowercase().contains("sunrise")
                    && t.to_lowercase().contains("reaping")),
            "expected sunrise book in {:?}",
            titles
        );
        assert!(
            !titles.iter().any(|t| t.contains("Final Strike")),
            "should not return homepage recent: {:?}",
            titles
        );
    }

    #[tokio::test]
    #[ignore]
    async fn live_search_paginates() {
        let client = AbbClient::new();
        let p1 = client.search("hunger games", 1).await.expect("p1");
        let p2 = client.search("hunger games", 2).await.expect("p2");
        assert!(!p1.results.is_empty());
        assert!(!p2.results.is_empty());
        assert_ne!(
            p1.results[0].url, p2.results[0].url,
            "page 2 should differ from page 1"
        );
        eprintln!(
            "p1={} p2={}",
            p1.results[0].title, p2.results[0].title
        );
    }

    #[tokio::test]
    #[ignore]
    async fn live_latest_feed() {
        let client = AbbClient::new();
        let page = client.latest(1).await.expect("latest");
        assert_eq!(page.mode, "latest");
        assert!(!page.results.is_empty());
        eprintln!(
            "latest n={} first={}",
            page.results.len(),
            page.results[0].title
        );
    }
}
