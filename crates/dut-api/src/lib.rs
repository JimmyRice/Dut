//! The HTTP surface: Axum routes, request and response DTOs, error mapping,
//! and request tracing.
//!
//! Handlers only extract input, call a service from `dut-core`, and map the
//! result to a response. The services' data sources are type parameters, so
//! this crate never depends on an adapter.

mod dto;
mod error;
mod http_cache;
mod middleware;
mod routes;
mod state;

use axum::{Router, extract::Request};
use dut_core::application::{line_status::LineStatusSource, next_train::NextTrainSource};

use crate::error::ApiError;

pub use state::AppState;

/// The whole HTTP surface: every route under `/api`, a JSON 404 for any
/// other path, and request tracing around both.
pub fn router<N: NextTrainSource, L: LineStatusSource>(state: AppState<N, L>) -> Router {
    let router = Router::new()
        .nest("/api", routes::router())
        .fallback(not_found)
        .with_state(state);
    middleware::with_request_tracing(router)
}

async fn not_found(request: Request) -> ApiError {
    ApiError::not_found(request.uri().path())
}
