use axum::{Router, response::Response, routing::get};

use crate::{
    api::{dto::LinesResponse, http_cache},
    state::AppState,
};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/lines", get(lines))
}

async fn lines() -> Response {
    http_cache::json(http_cache::reference_data(), LinesResponse::from_network())
}
