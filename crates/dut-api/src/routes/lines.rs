use axum::{Router, response::Response, routing::get};

use crate::{dto::LinesResponse, http_cache};

/// Serves reference data only, so it works with any router state.
pub(crate) fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/lines", get(lines))
}

async fn lines() -> Response {
    http_cache::json(http_cache::reference_data(), LinesResponse::from_network())
}
