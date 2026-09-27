pub mod dto;
pub mod error;

mod http_cache;
mod routes;

use axum::{Router, extract::Request};

use crate::{api::error::ApiError, state::AppState};

pub(crate) fn router() -> Router<AppState> {
    routes::router()
}

pub(crate) async fn not_found(request: Request) -> ApiError {
    ApiError::not_found(request.uri().path())
}
