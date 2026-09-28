//! Assembles the route groups into the application's router.

use axum::{Router, extract::Request};
use dut_core::application::{
    line_status::LineStatusSource, next_train::NextTrainSource, reference_data::ReferenceDataSource,
};
use tower_http::compression::CompressionLayer;

use crate::{error::ApiError, middleware, routes, state::AppState};

/// The whole HTTP surface: every route under `/api`, a JSON 404 for any
/// other path, and request tracing around both.
///
/// Responses are gzipped for clients that accept it: the MTR fare table is
/// about 1.7 MB of JSON and 75 KB compressed.
///
/// The health check is added after tracing, so it is not traced: probes
/// arrive every few seconds and would bury real requests in the log.
pub fn router<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>(
    state: AppState<N, L, R>,
) -> Router {
    let api = Router::new()
        .merge(routes::lines::router())
        .merge(routes::line_status::router())
        .merge(routes::next_train::router())
        .merge(routes::data::router());
    let router = Router::new()
        .nest("/api", api)
        .fallback(not_found)
        .layer(CompressionLayer::new())
        .with_state(state);
    middleware::with_request_tracing(router).merge(routes::health::router())
}

async fn not_found(request: Request) -> ApiError {
    ApiError::not_found(request.uri().path())
}
