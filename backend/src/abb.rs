use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::magnet::normalize_info_hash;

const ABB_MIRRORS: &[&str] = &[
    "https://audiobookbay.lu",
    "https://audiobookbay.fi",
];

#[derive(Debug, Clone, Serialize)]
pub struct AbbSearchResult {
    pub title: String,
    pub url: String,
    pub cover_url: Option<String>,
    pub info: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbbSearchPage {
    pub results: Vec<AbbSearchResult>,
    pub page: u32,
    pub has_more: bool,
    pub mirror: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbbDetails {
    pub title: String,
    pub url: String,
    pub info_hash: Option<String>,
    pub magnet_uri: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
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
                .user_agent(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
                )
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn search(&self, query: &str, page: u32) -> AppResult<AbbSearchPage> {
        let q = query.trim();
        if q.is_empty() {
            return Err(AppError::BadRequest("Search query required".into()));
        }
        let page = page.max(1);
        let encoded = urlencoding::encode(q);

        let mut last_err = None;
        for base in ABB_MIRRORS {
            let url = if page <= 1 {
                format!("{base}/?s={encoded}&cat=0%2C0")
            } else {
                // WordPress search pagination used by audiobookbay.lu
                format!("{base}/page/{page}/?s={encoded}&cat=0%2C0")
            };

            match self.fetch_html(&url).await {
                Ok(html) => {
                    let results = parse_search(&html, base);
                    let has_more = detect_has_more(&html, page) && !results.is_empty();
                    // Only accept empty as a valid "end of results" response from a live mirror.
                    // Do not fall through to another mirror on page>1 — that reloads page 1 results.
                    if !results.is_empty() || page > 1 {
                        return Ok(AbbSearchPage {
                            results,
                            page,
                            has_more,
                            mirror: (*base).to_string(),
                        });
                    }
                    last_err = Some(AppError::Internal(format!(
                        "AudiobookBay returned no parseable results from {base}"
                    )));
                }
                Err(err) => {
                    tracing::warn!(mirror = %base, error = %err, "ABB mirror failed");
                    last_err = Some(err);
                    // On page > 1, don't swap mirrors mid-pagination.
                    if page > 1 {
                        break;
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            AppError::Internal("All AudiobookBay mirrors failed".into())
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

        let html = self.fetch_html(&url).await?;
        let base = origin_of(&url).unwrap_or_else(|| ABB_MIRRORS[0].to_string());
        parse_details(&html, &url, &base).ok_or_else(|| {
            AppError::Internal(
                "Could not parse AudiobookBay page (site layout may have changed)".into(),
            )
        })
    }

    async fn fetch_html(&self, url: &str) -> AppResult<String> {
        let resp = self.http.get(url).send().await.map_err(|e| {
            AppError::Internal(format!("AudiobookBay request failed: {e}"))
        })?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "AudiobookBay returned {} for {url}",
                resp.status()
            )));
        }
        Ok(resp.text().await?)
    }
}

fn origin_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    Some(format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str()?
    ))
}

fn detect_has_more(html: &str, current_page: u32) -> bool {
    let next = current_page + 1;
    // wp-pagenavi style links: /page/N/?s=...
    if let Ok(re) = regex::Regex::new(r#"/page/(\d+)/\?s="#) {
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

fn parse_search(html: &str, base: &str) -> Vec<AbbSearchResult> {
    let document = Html::parse_document(html);
    let post_sel = match Selector::parse("div.post") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let title_sel = Selector::parse("div.postTitle h2 a, .postTitle h2 a").unwrap();
    let img_sel = Selector::parse("div.postContent img, .postContent img, img").unwrap();
    let info_sel = Selector::parse("div.postInfo, .postInfo, div.postContent").unwrap();

    let mut results = Vec::new();
    for post in document.select(&post_sel) {
        let Some(link) = post.select(&title_sel).next() else {
            continue;
        };
        let title = link.text().collect::<String>().trim().to_string();
        let href = link.value().attr("href").unwrap_or("").to_string();
        if title.is_empty() || href.is_empty() {
            continue;
        }
        if href.contains("/feed") || href.contains("/member/") || href.contains("/forum/") {
            continue;
        }

        let cover_url = post
            .select(&img_sel)
            .next()
            .and_then(|img| {
                img.value()
                    .attr("src")
                    .or_else(|| img.value().attr("data-src"))
            })
            .map(|s| absolutize(s, base));

        let info = post
            .select(&info_sel)
            .next()
            .map(|n| {
                n.text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.len() > 220 {
                    format!("{}…", &s[..217])
                } else {
                    s
                }
            });

        results.push(AbbSearchResult {
            title,
            url: absolutize(&href, base),
            cover_url,
            info,
        });
    }
    results
}

fn parse_details(html: &str, page_url: &str, base: &str) -> Option<AbbDetails> {
    let document = Html::parse_document(html);
    let title_sel = Selector::parse("div.postTitle h1, .postTitle h1, h1").ok()?;
    let title = document
        .select(&title_sel)
        .next()
        .map(|n| n.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "AudiobookBay title".into());

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

    let desc_sel = Selector::parse("div.postContent").ok()?;
    let description = document
        .select(&desc_sel)
        .next()
        .map(|n| {
            n.text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty());

    Some(AbbDetails {
        title,
        url: page_url.to_string(),
        info_hash,
        magnet_uri,
        cover_url,
        description,
    })
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
