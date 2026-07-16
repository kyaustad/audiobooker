mod abb;
mod api_key;
mod auth;
mod downloads;
mod libraries;
mod metadata;
mod push;
mod settings;
mod setup;
mod spa;
mod users;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use tower_http::{
    compression::CompressionLayer,
    trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer},
};
use tracing::Level;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let static_dir = state.config.static_dir.clone();

    let api = Router::new()
        .route("/health", get(health))
        .route("/setup/status", get(setup::status))
        .route("/setup", post(setup::create_root))
        .route("/setup/test-qbittorrent", post(setup::test_qbittorrent))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/password", put(auth::change_password))
        .route("/users", get(users::list).post(users::create))
        .route("/users/{id}", put(users::update).delete(users::delete))
        .route("/settings", get(settings::get).put(settings::update))
        .route("/settings/test-qbittorrent", post(settings::test_qbittorrent))
        .route("/settings/vapid", post(settings::ensure_vapid))
        .route("/settings/sync-abs-users", post(settings::sync_abs_users))
        .route("/libraries", get(libraries::list_all).post(libraries::create))
        .route("/libraries/mine", get(libraries::list_for_me))
        .route("/libraries/sync-abs", post(libraries::sync_from_abs))
        .route(
            "/libraries/{id}",
            put(libraries::update).delete(libraries::delete),
        )
        .route("/api-key", get(api_key::info).post(api_key::rotate))
        .route("/downloads", get(downloads::list).post(downloads::create))
        .route(
            "/downloads/{id}",
            get(downloads::get).delete(downloads::delete),
        )
        .route("/downloads/{id}/match", post(downloads::match_metadata))
        .route("/downloads/{id}/start-pack", post(downloads::start_pack))
        .route("/downloads/{id}/files", get(downloads::list_files))
        .route("/downloads/{id}/retry-imports", post(downloads::retry_pack_imports))
        .route(
            "/downloads/{id}/refresh-qbittorrent",
            post(downloads::refresh_qbittorrent),
        )
        .route("/downloads/{id}/items", post(downloads::map_item))
        .route(
            "/downloads/{id}/items/{item_id}",
            delete(downloads::unmap_item),
        )
        .route("/metadata/search", get(metadata::search))
        .route("/metadata/asin/{asin}", get(metadata::by_asin))
        .route("/push/vapid", get(push::vapid_public))
        .route("/push/subscribe", post(push::subscribe))
        .route("/push/status", get(push::status))
        .route("/push/preferences", put(push::update_preferences))
        .route("/push/unsubscribe", post(push::unsubscribe))
        .route("/push/test", post(push::test_push))
        .route("/abb/browse", get(abb::browse))
        .route("/abb/categories", get(abb::categories))
        .route("/abb/search", get(abb::search))
        .route("/abb/details", get(abb::details))
        .route("/v1/user", post(users::create))
        .route("/v1/queue", get(downloads::list_all_for_api))
        .route("/v1/queue/{username}", get(downloads::list_for_username));

    let _ = static_dir;

    Router::new()
        .nest("/api", api)
        .fallback(spa::fallback)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .layer(CompressionLayer::new())
        .with_state(state)
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "ok": true, "version": "2.0.0" }))
}
