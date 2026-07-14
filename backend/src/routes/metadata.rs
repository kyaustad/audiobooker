use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::models::Settings;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub title: String,
    pub author: Option<String>,
}

pub async fn search(
    State(state): State<AppState>,
    _auth: AuthSession,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    if query.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    let matches = state
        .metadata
        .search(
            &settings.metadata_provider_url,
            &settings.audible_region,
            query.title.trim(),
            query.author.as_deref(),
        )
        .await?;
    Ok(Json(json!({ "matches": matches })))
}

pub async fn by_asin(
    State(state): State<AppState>,
    _auth: AuthSession,
    Path(asin): Path<String>,
) -> AppResult<Json<Value>> {
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    let book = state
        .metadata
        .get_by_asin(
            &settings.metadata_provider_url,
            &settings.audible_region,
            &asin,
        )
        .await?;
    Ok(Json(json!({ "match": book })))
}
