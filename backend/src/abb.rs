use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::magnet::normalize_info_hash;

/// Only include mirrors that currently serve usable HTML over HTTPS.
/// Mixing mirrors is the main reason search order diverges from the site.
const ABB_MIRRORS: &[&str] = &["https://audiobookbay.lu"];

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
    /// `latest` (homepage feed) or `search`
    pub mode: String,
    pub query: Option<String>,
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

    /// Homepage / “recent uploads” feed (Overseerr-style Discover default).
    pub async fn latest(&self, page: u32) -> AppResult<AbbSearchPage> {
        let page = page.max(1);
        let base = ABB_MIRRORS[0];
        let _ = self.warm_session(base).await;

        let url = if page <= 1 {
            format!("{base}/")
        } else {
            format!("{base}/page/{page}/")
        };

        let html = self.fetch_html(&url, base).await?;
        let results = parse_listing(&html, base);
        let has_more = detect_has_more(&html, page) && !results.is_empty();
        if results.is_empty() && page == 1 {
            return Err(AppError::Internal(
                "Could not parse AudiobookBay homepage feed".into(),
            ));
        }
        Ok(AbbSearchPage {
            results,
            page,
            has_more,
            mirror: base.to_string(),
            mode: "latest".into(),
            query: None,
        })
    }

    pub async fn search(&self, query: &str, page: u32) -> AppResult<AbbSearchPage> {
        let q = query.trim();
        if q.is_empty() {
            return Err(AppError::BadRequest("Search query required".into()));
        }
        let page = page.max(1);
        let encoded = encode_abb_query(q);
        let base = ABB_MIRRORS[0];
        let _ = self.warm_session(base).await;

        // Try the exact browser form first, then simpler variants ABB also accepts.
        let url_candidates: Vec<String> = if page <= 1 {
            vec![
                format!("{base}/?s={encoded}&cat=undefined%2Cundefined"),
                format!("{base}/?s={encoded}&cat=0%2C0"),
                format!("{base}/?s={encoded}"),
            ]
        } else {
            vec![
                format!("{base}/page/{page}/?s={encoded}&cat=undefined%2Cundefined"),
                format!("{base}/page/{page}/?s={encoded}&cat=0%2C0"),
                format!("{base}/page/{page}/?s={encoded}"),
            ]
        };

        let mut last_err = None;
        for url in url_candidates {
            tracing::info!(%url, "ABB search fetch");
            match self.fetch_html(&url, base).await {
                Ok(html) => {
                    if is_abb_homepage(&html) && !is_abb_search_results_page(&html) {
                        tracing::warn!(%url, "ABB search URL returned homepage HTML; trying next variant");
                        last_err = Some(AppError::Internal(
                            "AudiobookBay ignored the search query (got homepage). Retrying…".into(),
                        ));
                        // Re-warm and continue
                        let _ = self.warm_session(base).await;
                        continue;
                    }

                    let results = parse_listing(&html, base);
                    let has_more = detect_has_more(&html, page) && !results.is_empty();
                    if !results.is_empty() || page > 1 {
                        return Ok(AbbSearchPage {
                            results,
                            page,
                            has_more,
                            mirror: base.to_string(),
                            mode: "search".into(),
                            query: Some(q.to_string()),
                        });
                    }
                    last_err = Some(AppError::Internal(
                        "No AudiobookBay results matched that search".into(),
                    ));
                }
                Err(err) => {
                    tracing::warn!(error = %err, %url, "ABB search request failed");
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            AppError::Internal("AudiobookBay search failed".into())
        }))
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
        let _ = self.warm_session(&base).await;
        let html = self.fetch_html(&url, &base).await?;
        parse_details(&html, &url, &base).ok_or_else(|| {
            AppError::Internal(
                "Could not parse AudiobookBay page (site layout may have changed)".into(),
            )
        })
    }

    async fn warm_session(&self, base: &str) -> AppResult<()> {
        // ABB sets PHPSESSID on first visit; search without it can bounce to the homepage.
        let _ = self
            .http
            .get(format!("{base}/"))
            .header("Accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await?;
        Ok(())
    }

    async fn fetch_html(&self, url: &str, origin: &str) -> AppResult<String> {
        let resp = self
            .http
            .get(url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
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
    // WordPress search uses application/x-www-form-urlencoded (+ for spaces).
    urlencoding::encode(q)
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

fn is_abb_homepage(html: &str) -> bool {
    let title = html
        .split("<title>")
        .nth(1)
        .and_then(|s| s.split("</title>").next())
        .unwrap_or("")
        .to_ascii_lowercase();
    title.contains("unabridged audiobooks free download") && !is_abb_search_results_page(html)
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
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_search_matches_abb() {
        let client = AbbClient::new();
        let page = client.search("sunrise on the reaping", 1).await.expect("search");
        let titles: Vec<_> = page.results.iter().map(|r| r.title.clone()).collect();
        eprintln!("mirror={} n={} titles={:?}", page.mirror, titles.len(), titles);
        assert_eq!(page.mode, "search");
        assert!(
            titles.iter().any(|t| t.to_lowercase().contains("sunrise") && t.to_lowercase().contains("reaping")),
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
    async fn live_latest_feed() {
        let client = AbbClient::new();
        let page = client.latest(1).await.expect("latest");
        assert_eq!(page.mode, "latest");
        assert!(!page.results.is_empty());
        eprintln!("latest n={} first={}", page.results.len(), page.results[0].title);
    }
}
