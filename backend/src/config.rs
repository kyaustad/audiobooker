use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_path: PathBuf,
    pub static_dir: PathBuf,
    pub cookie_secure: bool,
    pub session_ttl_hours: i64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            database_path: PathBuf::from(
                env::var("DATABASE_PATH").unwrap_or_else(|_| "data/audiobooker.db".into()),
            ),
            static_dir: PathBuf::from(
                env::var("STATIC_DIR").unwrap_or_else(|_| "static".into()),
            ),
            cookie_secure: env::var("COOKIE_SECURE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            session_ttl_hours: env::var("SESSION_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(168),
        }
    }
}
