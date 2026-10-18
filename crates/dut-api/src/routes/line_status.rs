use axum::{Router, extract::State, http::HeaderMap, response::Response, routing::get};
use dut_core::application::{
    line_status::LineStatusSource, next_train::NextTrainSource, reference_data::ReferenceDataSource,
};

use crate::{
    body_version::FetchVersion,
    dto::LineStatusResponse,
    encoded_body::{JSON, render_json},
    error::ApiError,
    http_cache,
    state::AppState,
};

pub(crate) fn router<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>()
-> Router<AppState<N, L, R>> {
    Router::new().route("/lines/status", get(line_status::<N, L, R>))
}

/// Every request between two polls gets the same body, so it is encoded
/// once per poll and shared.
async fn line_status<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.line_status().status().await?;
    let cache_control = http_cache::for_freshness(snapshot.freshness());
    let version = FetchVersion::of(&snapshot);
    let body = state
        .line_status_body()
        .respond(&headers, version, JSON, move || {
            render_json(&LineStatusResponse::from(&snapshot))
        });
    Ok(http_cache::with_cache_control(cache_control, body).await)
}
