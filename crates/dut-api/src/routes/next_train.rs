use axum::{
    Router,
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    routing::get,
};
use dut_core::{
    application::{
        line_status::LineStatusSource, next_train::NextTrainSource,
        reference_data::ReferenceDataSource,
    },
    domain::network::{Line, StationCode},
};

use crate::{
    body_version::{BoardVersion, StationBoardsVersion},
    dto::{NextTrainResponse, StationBoardsResponse},
    encoded_body::{EncodedBodies, JSON, render_json},
    error::ApiError,
    http_cache,
    routes::params,
    state::AppState,
};

/// The Next Train bodies, encoded once per version and shared by the
/// requests for them. Many riders ask for the same station between two
/// fetches of its board, and each would otherwise serialize and gzip it.
///
/// Keys are added only after the service has checked them against the
/// static network, so there is at most one body per line and station.
#[derive(Debug)]
pub(crate) struct NextTrainBodies {
    boards: EncodedBodies<(Line, StationCode), BoardVersion>,
    station_boards: EncodedBodies<StationCode, StationBoardsVersion>,
}

impl Default for NextTrainBodies {
    fn default() -> Self {
        Self {
            boards: EncodedBodies::new("next-train-board"),
            station_boards: EncodedBodies::new("next-train-station-boards"),
        }
    }
}

pub(crate) fn router<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>()
-> Router<AppState<N, L, R>> {
    Router::new()
        .route(
            "/lines/{line}/stations/{station}/next-trains",
            get(line_board::<N, L, R>),
        )
        .route(
            "/stations/{station}/next-trains",
            get(station_boards::<N, L, R>),
        )
}

async fn line_board<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    Path((line, station)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let line = params::line(&line)?;
    let station = params::station(&station)?;
    let view = state.next_trains().board(line, station).await?;
    let cache_control = http_cache::for_freshness(view.snapshot().freshness());
    let version = BoardVersion::of(&view);
    let body = state.next_train_bodies().boards.get((line, station));
    let response = body.respond(&headers, version, JSON, move || {
        render_json(&NextTrainResponse::from(&view))
    });
    Ok(http_cache::with_cache_control(cache_control, response).await)
}

async fn station_boards<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    Path(station): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let station = params::station(&station)?;
    let boards = state.next_trains().station_boards(station).await?;
    let cache_control = http_cache::for_freshness(boards.freshness());
    let version = StationBoardsVersion::of(&boards);
    let body = state.next_train_bodies().station_boards.get(station);
    let response = body.respond(&headers, version, JSON, move || {
        let unavailable = ApiError::UpstreamUnavailable.detail();
        render_json(&StationBoardsResponse::new(&boards, &unavailable))
    });
    Ok(http_cache::with_cache_control(cache_control, response).await)
}
