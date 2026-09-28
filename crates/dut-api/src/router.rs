//! Assembles the route groups into the application's router.

use axum::{Router, extract::Request};
use dut_core::application::{line_status::LineStatusSource, next_train::NextTrainSource};

use crate::{error::ApiError, middleware, routes, state::AppState};

/// The whole HTTP surface: every route under `/api`, a JSON 404 for any
/// other path, and request tracing around both.
pub fn router<N: NextTrainSource, L: LineStatusSource>(state: AppState<N, L>) -> Router {
    let api = Router::new()
        .merge(routes::lines::router())
        .merge(routes::line_status::router())
        .merge(routes::next_train::router());
    let router = Router::new()
        .nest("/api", api)
        .fallback(not_found)
        .with_state(state);
    middleware::with_request_tracing(router)
}

async fn not_found(request: Request) -> ApiError {
    ApiError::not_found(request.uri().path())
}
