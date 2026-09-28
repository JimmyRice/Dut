use axum::{
    Router,
    extract::{Path, State},
    response::Response,
    routing::get,
};
use dut_core::application::{line_status::LineStatusSource, next_train::NextTrainSource};

use crate::{
    dto::{NextTrainResponse, StationBoardsResponse},
    error::ApiError,
    http_cache,
    routes::params,
    state::AppState,
};

pub(super) fn router<N: NextTrainSource, L: LineStatusSource>() -> Router<AppState<N, L>> {
    Router::new()
        .route(
            "/lines/{line}/stations/{station}/next-trains",
            get(line_board::<N, L>),
        )
        .route(
            "/stations/{station}/next-trains",
            get(station_boards::<N, L>),
        )
}

async fn line_board<N: NextTrainSource, L: LineStatusSource>(
    State(state): State<AppState<N, L>>,
    Path((line, station)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let line = params::line(&line)?;
    let station = params::station(&station)?;
    let view = state.next_trains().board(line, station).await?;

    Ok(http_cache::json(
        http_cache::for_freshness(view.snapshot().freshness()),
        NextTrainResponse::from(&view),
    ))
}

async fn station_boards<N: NextTrainSource, L: LineStatusSource>(
    State(state): State<AppState<N, L>>,
    Path(station): Path<String>,
) -> Result<Response, ApiError> {
    let station = params::station(&station)?;
    let boards = state.next_trains().station_boards(station).await?;
    let unavailable = ApiError::UpstreamUnavailable.detail();

    Ok(http_cache::json(
        http_cache::for_freshness(boards.freshness()),
        StationBoardsResponse::new(&boards, &unavailable),
    ))
}
