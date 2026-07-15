use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::abb::AbbClient;
use crate::auth::AuthSession;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub page: Option<u32>,
}

#[derive(Deserialize)]
pub struct BrowseQuery {
    pub page: Option<u32>,
    /// ABB `/audio-books/type/{slug}/` category when set.
    pub category: Option<String>,
}

#[derive(Deserialize)]
pub struct DetailsQuery {
    pub url: String,
}

fn page_json(page: crate::abb::AbbSearchPage) -> Json<Value> {
    Json(json!({
        "results": page.results,
        "page": page.page,
        "has_more": page.has_more,
        "mirror": page.mirror,
        "mode": page.mode,
        "query": page.query,
        "category": page.category,
        "category_label": page.category_label,
    }))
}

pub async fn categories(auth: AuthSession) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    Ok(Json(json!({ "categories": AbbClient::categories() })))
}

pub async fn browse(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<BrowseQuery>,
) -> AppResult<Json<Value>> {
    if auth.user.is_root() {
        return Err(AppError::Forbidden);
    }
    let page_n = query.page.unwrap_or(1);
    let page = if let Some(cat) = query.category.as_deref().map(str::trim).filter(|s| !s.is_empty())
    {
        state.abb.category(cat, page_n).await?
    } else {
        state.abb.latest(page_n).await?
    };
    Ok(page_json(page))
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
    Ok(page_json(page))
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
