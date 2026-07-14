use std::path::PathBuf;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

pub async fn fallback(State(state): State<AppState>, uri: Uri, req: Request<Body>) -> Response {
    spa_handler(state.config.static_dir.clone(), uri, req).await
}

async fn spa_handler(static_dir: PathBuf, uri: Uri, req: Request<Body>) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    let file_path = if path.is_empty() {
        static_dir.join("index.html")
    } else {
        static_dir.join(path)
    };

    if path != "" && file_path.is_file() {
        return match ServeDir::new(&static_dir).oneshot(req).await {
            Ok(res) => res.into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Static file error").into_response(),
        };
    }

    let index = static_dir.join("index.html");
    if index.is_file() {
        match ServeFile::new(index)
            .oneshot(Request::new(Body::empty()))
            .await
        {
            Ok(mut res) => {
                res.headers_mut().insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-cache"),
                );
                res.into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Missing index.html").into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Frontend not built. Run pnpm build in frontend/ and copy dist to backend/static.",
        )
            .into_response()
    }
}
