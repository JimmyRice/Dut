use axum::{Router, extract::State, response::Response, routing::get};

use crate::{
    api::{dto::LineStatusResponse, error::ApiError, http_cache},
    state::AppState,
};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/lines/status", get(line_status))
}

async fn line_status(State(state): State<AppState>) -> Result<Response, ApiError> {
    let snapshot = state.line_status().status().await?;

    Ok(http_cache::json(
        http_cache::for_freshness(snapshot.freshness()),
        LineStatusResponse::from(&snapshot),
    ))
}
