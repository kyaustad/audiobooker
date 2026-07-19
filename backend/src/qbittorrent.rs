use reqwest::{Client, header::SET_COOKIE};
use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct QbTorrent {
    pub hash: String,
    pub name: String,
    pub progress: f64,
    pub dlspeed: i64,
    pub eta: i64,
    pub state: String,
    pub save_path: String,
    #[serde(default)]
    pub content_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QbTorrentFile {
    pub name: String,
    #[serde(default)]
    pub size: i64,
}

#[derive(Clone)]
pub struct QbittorrentClient {
    http: Client,
}

impl Default for QbittorrentClient {
    fn default() -> Self {
        Self::new()
    }
}

impl QbittorrentClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .cookie_store(false)
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
        }
    }

    async fn login_cookie(&self, base: &str, username: &str, password: &str) -> AppResult<String> {
        let url = format!("{}/api/v2/auth/login", base.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .form(&[("username", username), ("password", password)])
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!(
                    "Cannot reach qBittorrent at {base} ({e}). Check URL and network."
                ))
            })?;

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "qBittorrent login failed ({})",
                resp.status()
            )));
        }

        let sid = resp
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|c| {
                c.split(';')
                    .next()
                    .filter(|p| p.starts_with("SID="))
                    .map(|p| p.to_string())
            });

        let body = resp.text().await.unwrap_or_default();
        if body.trim() == "Fails." {
            return Err(AppError::BadRequest(
                "qBittorrent login failed: invalid credentials".into(),
            ));
        }

        sid.ok_or_else(|| {
            AppError::Internal(
                "qBittorrent login failed: missing session cookie. Is the Web UI enabled?".into(),
            )
        })
    }

    pub async fn test_connection(
        &self,
        base: &str,
        username: &str,
        password: &str,
    ) -> AppResult<()> {
        let _ = self.login_cookie(base, username, password).await?;
        Ok(())
    }

    pub async fn add_magnet(
        &self,
        base: &str,
        username: &str,
        password: &str,
        magnet: &str,
        category: Option<&str>,
        tags: Option<&str>,
    ) -> AppResult<()> {
        let cookie = self.login_cookie(base, username, password).await?;
        let url = format!("{}/api/v2/torrents/add", base.trim_end_matches('/'));
        let mut form = vec![("urls", magnet.to_string())];
        if let Some(cat) = category {
            form.push(("category", cat.to_string()));
        }
        // Comma-separated; tags that do not exist are created automatically.
        if let Some(tag) = tags.map(str::trim).filter(|t| !t.is_empty()) {
            form.push(("tags", tag.to_string()));
        }
        let resp = self
            .http
            .post(&url)
            .header("Cookie", &cookie)
            .form(&form)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "qBittorrent rejected magnet add ({})",
                resp.status()
            )));
        }
        Ok(())
    }

    pub async fn list_torrents(
        &self,
        base: &str,
        username: &str,
        password: &str,
    ) -> AppResult<Vec<QbTorrent>> {
        let cookie = self.login_cookie(base, username, password).await?;
        let url = format!("{}/api/v2/torrents/info", base.trim_end_matches('/'));
        let resp = self
            .http
            .get(&url)
            .header("Cookie", &cookie)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "Failed to fetch torrents ({})",
                resp.status()
            )));
        }
        Ok(resp.json().await?)
    }

    pub async fn torrent_files(
        &self,
        base: &str,
        username: &str,
        password: &str,
        hash: &str,
    ) -> AppResult<Vec<QbTorrentFile>> {
        let cookie = self.login_cookie(base, username, password).await?;
        let url = format!(
            "{}/api/v2/torrents/files?hash={}",
            base.trim_end_matches('/'),
            urlencoding::encode(hash)
        );
        let resp = self
            .http
            .get(&url)
            .header("Cookie", &cookie)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "Failed to fetch torrent files ({})",
                resp.status()
            )));
        }
        Ok(resp.json().await?)
    }

    pub async fn delete_torrent(
        &self,
        base: &str,
        username: &str,
        password: &str,
        hash: &str,
        delete_files: bool,
    ) -> AppResult<()> {
        let cookie = self.login_cookie(base, username, password).await?;
        let url = format!("{}/api/v2/torrents/delete", base.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .header("Cookie", &cookie)
            .form(&[
                ("hashes", hash),
                ("deleteFiles", if delete_files { "true" } else { "false" }),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "Failed to delete torrent ({})",
                resp.status()
            )));
        }
        Ok(())
    }
}

pub fn map_state(state: &str, progress: f64) -> &'static str {
    if progress >= 1.0 {
        return "completed";
    }
    match state {
        "error" | "missingFiles" => "error",
        "queuedDL" | "checkingDL" | "stalledDL" | "downloading" | "metaDL" | "forcedDL" => {
            "downloading"
        }
        _ => "queued",
    }
}
