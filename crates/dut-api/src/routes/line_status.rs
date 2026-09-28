use axum::{Router, extract::State, response::Response, routing::get};
use dut_core::application::{line_status::LineStatusSource, next_train::NextTrainSource};

use crate::{dto::LineStatusResponse, error::ApiError, http_cache, state::AppState};

pub(crate) fn router<N: NextTrainSource, L: LineStatusSource>() -> Router<AppState<N, L>> {
    Router::new().route("/lines/status", get(line_status::<N, L>))
}

async fn line_status<N: NextTrainSource, L: LineStatusSource>(
    State(state): State<AppState<N, L>>,
) -> Result<Response, ApiError> {
    let snapshot = state.line_status().status().await?;

    Ok(http_cache::json(
        http_cache::for_freshness(snapshot.freshness()),
        LineStatusResponse::from(&snapshot),
    ))
}
