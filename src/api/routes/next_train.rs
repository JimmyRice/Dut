use axum::{
    Router,
    extract::{Path, State},
    response::Response,
    routing::get,
};

use crate::{
    api::{
        dto::{NextTrainResponse, StationBoardsResponse},
        error::ApiError,
        http_cache,
        routes::params,
    },
    state::AppState,
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/lines/{line}/stations/{station}/next-trains",
            get(line_board),
        )
        .route("/stations/{station}/next-trains", get(station_boards))
}

async fn line_board(
    State(state): State<AppState>,
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

async fn station_boards(
    State(state): State<AppState>,
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
