use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub page: Option<u32>,
}

#[derive(Deserialize)]
pub struct DetailsQuery {
    pub url: String,
}

pub async fn search(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let page = state.abb.search(&query.q, query.page.unwrap_or(1)).await?;
    Ok(Json(json!({
        "results": page.results,
        "page": page.page,
        "has_more": page.has_more,
        "mirror": page.mirror,
    })))
}

pub async fn details(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<DetailsQuery>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let details = state.abb.details(&query.url).await?;
    Ok(Json(json!({ "details": details })))
}
