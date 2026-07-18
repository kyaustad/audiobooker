use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const AUDIBLE_REGION_TLD: &[(&str, &str)] = &[
    ("us", ".com"),
    ("ca", ".ca"),
    ("uk", ".co.uk"),
    ("au", ".com.au"),
    ("fr", ".fr"),
    ("de", ".de"),
    ("jp", ".co.jp"),
    ("it", ".it"),
    ("in", ".in"),
    ("es", ".es"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataMatch {
    pub asin: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub narrators: Vec<String>,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub series_index: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub region: String,
}

#[derive(Clone)]
pub struct MetadataClient {
    http: Client,
}

impl Default for MetadataClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(25))
                .user_agent("Audiobooker/2.0 (Audiobookshelf-compatible metadata)")
                .build()
                .expect("reqwest client"),
        }
    }

    fn tld_for_region(region: &str) -> &'static str {
        AUDIBLE_REGION_TLD
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(region))
            .map(|(_, tld)| *tld)
            .unwrap_or(".com")
    }

    fn is_asin(value: &str) -> bool {
        let v = value.trim();
        v.len() == 10
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric())
    }

    /// ABS-style search:
    /// 1) ASIN via Audnexus enrichment
    /// 2) Audible catalog product search → enrich each ASIN via Audnexus
    pub async fn search(
        &self,
        provider_base: &str,
        region: &str,
        title: &str,
        author: Option<&str>,
    ) -> AppResult<Vec<MetadataMatch>> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::BadRequest("title is required".into()));
        }

        let region = if region.trim().is_empty() {
            "us"
        } else {
            region.trim()
        };

        // Direct ASIN in title field
        if Self::is_asin(title) {
            if let Ok(book) = self.get_by_asin(provider_base, region, title).await {
                return Ok(vec![book]);
            }
        }

        let asins = self
            .audible_catalog_asins(region, title, author)
            .await
            .unwrap_or_default();

        let futures = asins.into_iter().take(10).map(|asin| {
            let this = self.clone();
            let provider = provider_base.to_string();
            let region = region.to_string();
            async move { this.get_by_asin(&provider, &region, &asin).await.ok() }
        });
        let matches = futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        if matches.is_empty() {
            return Ok(Vec::new());
        }

        Ok(matches)
    }

    pub async fn get_by_asin(
        &self,
        provider_base: &str,
        region: &str,
        asin: &str,
    ) -> AppResult<MetadataMatch> {
        let asin = asin.trim().to_uppercase();
        if !Self::is_asin(&asin) {
            return Err(AppError::BadRequest("ASIN must be 10 alphanumeric characters".into()));
        }

        let region = if region.trim().is_empty() {
            "us"
        } else {
            region.trim()
        };

        let base = provider_base.trim_end_matches('/');
        let url = format!("{base}/books/{asin}?region={region}");
        let resp = self.http.get(&url).send().await.map_err(|e| {
            AppError::Internal(format!("Audnexus request failed: {e}"))
        })?;

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "Audnexus returned {} for ASIN {asin}",
                resp.status()
            )));
        }

        let value: serde_json::Value = resp.json().await?;
        parse_book(&value, region).ok_or_else(|| {
            AppError::Internal(format!("Could not parse Audnexus response for ASIN {asin}"))
        })
    }

    async fn audible_catalog_asins(
        &self,
        region: &str,
        title: &str,
        author: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let tld = Self::tld_for_region(region);
        let mut url = url::Url::parse(&format!(
            "https://api.audible{tld}/1.0/catalog/products"
        ))
        .map_err(AppError::internal)?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("num_results", "10");
            q.append_pair("products_sort_by", "Relevance");
            q.append_pair("title", title);
            if let Some(a) = author.map(str::trim).filter(|s| !s.is_empty()) {
                q.append_pair("author", a);
            }
        }

        let resp = self.http.get(url).send().await.map_err(|e| {
            AppError::Internal(format!("Audible catalog search failed: {e}"))
        })?;

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "Audible catalog search returned {}",
                resp.status()
            )));
        }

        let value: serde_json::Value = resp.json().await?;
        let products = value
            .get("products")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let asins = products
            .iter()
            .filter_map(|p| p.get("asin").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect::<Vec<_>>();

        if asins.is_empty() {
            return Err(AppError::Internal(
                "Audible catalog returned no products".into(),
            ));
        }

        Ok(asins)
    }
}

fn parse_book(value: &serde_json::Value, region: &str) -> Option<MetadataMatch> {
    let asin = value
        .get("asin")
        .or_else(|| value.get("ASIN"))
        .and_then(|v| v.as_str())?
        .to_string();
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let authors = extract_names(value, &["authors", "author"]);
    let narrators = extract_names(value, &["narrators", "narrator"]);

    let (series, series_index) = extract_series(value);

    let cover_url = value
        .get("image")
        .or_else(|| value.get("cover"))
        .or_else(|| value.get("coverUrl"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let description = value
        .get("description")
        .or_else(|| value.get("summary"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let subtitle = value
        .get("subtitle")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(MetadataMatch {
        asin,
        title,
        subtitle,
        authors,
        narrators,
        series,
        series_index,
        cover_url,
        description,
        region: region.to_string(),
    })
}

fn extract_series(value: &serde_json::Value) -> (Option<String>, Option<String>) {
    if let Some(primary) = value.get("seriesPrimary") {
        let name = primary.get("name").and_then(|n| n.as_str()).map(str::to_string);
        let index = primary
            .get("position")
            .and_then(|p| {
                p.as_str()
                    .map(str::to_string)
                    .or_else(|| p.as_f64().map(|n| n.to_string()))
            })
            .map(|s| clean_sequence(&s));
        return (name, index);
    }

    match value.get("series") {
        Some(s) if s.is_string() => (s.as_str().map(str::to_string), None),
        Some(s) if s.is_object() => {
            let name = s.get("name").and_then(|n| n.as_str()).map(str::to_string);
            let index = s
                .get("position")
                .or_else(|| s.get("index"))
                .and_then(|p| {
                    p.as_str()
                        .map(str::to_string)
                        .or_else(|| p.as_f64().map(|n| n.to_string()))
                })
                .map(|x| clean_sequence(&x));
            (name, index)
        }
        Some(s) if s.is_array() => {
            let first = s.as_array().and_then(|arr| arr.first());
            let name = first
                .and_then(|f| f.get("name").or_else(|| f.get("series")))
                .and_then(|n| n.as_str())
                .map(str::to_string);
            let index = first
                .and_then(|f| f.get("sequence").or_else(|| f.get("position")).or_else(|| f.get("index")))
                .and_then(|p| {
                    p.as_str()
                        .map(str::to_string)
                        .or_else(|| p.as_f64().map(|n| n.to_string()))
                })
                .map(|x| clean_sequence(&x));
            (name, index)
        }
        _ => (None, None),
    }
}

fn clean_sequence(sequence: &str) -> String {
    let re = regex::Regex::new(r"(?:\.\d+|\d+(?:\.\d+)?)").ok();
    if let Some(re) = re {
        if let Some(m) = re.find(sequence) {
            return m.as_str().to_string();
        }
    }
    sequence.to_string()
}

fn extract_names(value: &serde_json::Value, keys: &[&str]) -> Vec<String> {
    for key in keys {
        if let Some(v) = value.get(*key) {
            if let Some(s) = v.as_str() {
                return s
                    .split(',')
                    .map(|part| part.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if let Some(arr) = v.as_array() {
                return arr
                    .iter()
                    .filter_map(|item| {
                        if let Some(s) = item.as_str() {
                            Some(s.to_string())
                        } else {
                            item.get("name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        }
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}
