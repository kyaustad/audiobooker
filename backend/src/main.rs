mod abb;
mod abb_cache;
mod abs_users;
mod auth;
mod config;
mod db;
mod error;
mod files;
mod limits;
mod magnet;
mod metadata;
mod models;
mod push;
mod qbittorrent;
mod routes;
mod state;
mod worker;

use std::net::SocketAddr;

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::abb::AbbClient;
use crate::config::Config;
use crate::metadata::MetadataClient;
use crate::qbittorrent::QbittorrentClient;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,audiobooker=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let pool = db::connect(&config.database_path).await?;
    tracing::info!(path = %config.database_path.display(), "database ready");

    let qb = QbittorrentClient::new();
    worker::spawn_worker(pool.clone(), qb.clone());

    let state = AppState {
        pool: pool.clone(),
        config: config.clone(),
        qb,
        metadata: MetadataClient::new(),
        abb: AbbClient::new(pool),
    };

    let app = routes::router(state);
    let addr = SocketAddr::new(config.host.parse()?, config.port);
    tracing::info!("Audiobooker v2 listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
