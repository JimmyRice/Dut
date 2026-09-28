use axum::{Router, extract::State, response::Response, routing::get};
use dut_core::application::{
    line_status::LineStatusSource, next_train::NextTrainSource, reference_data::ReferenceDataSource,
};

use crate::{dto::LineStatusResponse, error::ApiError, http_cache, state::AppState};

pub(crate) fn router<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>()
-> Router<AppState<N, L, R>> {
    Router::new().route("/lines/status", get(line_status::<N, L, R>))
}

async fn line_status<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
) -> Result<Response, ApiError> {
    let snapshot = state.line_status().status().await?;

    Ok(http_cache::json(
        http_cache::for_freshness(snapshot.freshness()),
        LineStatusResponse::from(&snapshot),
    ))
}
