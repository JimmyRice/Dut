pub mod dto;
pub mod error;

mod http_cache;
mod middleware;
mod routes;

use axum::{Router, extract::Request};

use crate::{api::error::ApiError, state::AppState};

/// The whole HTTP surface: every route under `/api`, a JSON 404 for any
/// other path, and request tracing around both.
pub(crate) fn router(state: AppState) -> Router {
    let router = Router::new()
        .nest("/api", routes::router())
        .fallback(not_found)
        .with_state(state);
    middleware::with_request_tracing(router)
}

async fn not_found(request: Request) -> ApiError {
    ApiError::not_found(request.uri().path())
}
