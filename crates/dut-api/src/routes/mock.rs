//! The mock API: the real-time endpoints' responses, simulated in a named or
//! random scenario, for developing the app. Each route mirrors the real one
//! under `/api/mock` and answers with the same body, headers, and errors, so
//! the app reaches it by changing its path prefix alone.
//!
//! Every simulated response says which scenario and seed produced it, in
//! `x-mock-scenario` and `x-mock-seed`, so a client can request it again.

use axum::{
    Router,
    extract::{Path, Query, rejection::QueryRejection},
    http::{HeaderName, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
};
use tracing::debug;

use dut_mock::{
    BoardScenario, Scenario, Seed, SimulatedLineStatus, SimulatedNextTrains, StatusScenario,
};

use crate::{
    dto::{
        LineStatusResponse, MockQuery, NextTrainResponse, ScenariosResponse, StationBoardsResponse,
    },
    error::ApiError,
    http_cache,
    routes::{mock_events, params},
};

const SCENARIO_HEADER: HeaderName = HeaderName::from_static("x-mock-scenario");
const SEED_HEADER: HeaderName = HeaderName::from_static("x-mock-seed");

/// Simulates data only, so it works with any router state.
pub(crate) fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/scenarios", get(scenarios))
        .route("/lines/status", get(line_status))
        .route(
            "/lines/{line}/stations/{station}/next-trains",
            get(line_board),
        )
        .route("/stations/{station}/next-trains", get(station_boards))
        .route("/events", get(mock_events::events))
}

async fn scenarios() -> Response {
    http_cache::json(http_cache::reference_data(), ScenariosResponse::catalogue())
}

async fn line_status(
    query: Result<Query<MockQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (scenario, seed) = params::scenario::<StatusScenario>(query)?;
    let response = SimulatedLineStatus::new(scenario, seed)
        .status()
        .map(|snapshot| {
            http_cache::json(
                http_cache::for_freshness(snapshot.freshness()),
                LineStatusResponse::from(&snapshot),
            )
        })
        .map_err(ApiError::from);

    Ok(simulated(scenario, seed, response))
}

async fn line_board(
    Path((line, station)): Path<(String, String)>,
    query: Result<Query<MockQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let line = params::line(&line)?;
    let station = params::station(&station)?;
    let (scenario, seed) = params::scenario::<BoardScenario>(query)?;
    let response = SimulatedNextTrains::new(scenario, seed)
        .board(line, station)
        .await
        .map(|view| {
            http_cache::json(
                http_cache::for_freshness(view.snapshot().freshness()),
                NextTrainResponse::from(&view),
            )
        })
        .map_err(ApiError::from);

    Ok(simulated(scenario, seed, response))
}

async fn station_boards(
    Path(station): Path<String>,
    query: Result<Query<MockQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let station = params::station(&station)?;
    let (scenario, seed) = params::scenario::<BoardScenario>(query)?;
    let unavailable = ApiError::UpstreamUnavailable.detail();
    let response = SimulatedNextTrains::new(scenario, seed)
        .station_boards(station)
        .await
        .map(|boards| {
            http_cache::json(
                http_cache::for_freshness(boards.freshness()),
                StationBoardsResponse::new(&boards, &unavailable),
            )
        })
        .map_err(ApiError::from);

    Ok(simulated(scenario, seed, response))
}

/// Labels a response, error or not, with the scenario and seed it was
/// simulated in.
pub(super) fn simulated<S: Scenario>(
    scenario: S,
    seed: Seed,
    response: Result<Response, ApiError>,
) -> Response {
    debug!(scenario = scenario.name(), %seed, "simulated a response");
    let mut response = response.into_response();
    let headers = response.headers_mut();
    headers.insert(SCENARIO_HEADER, HeaderValue::from_static(scenario.name()));
    headers.insert(SEED_HEADER, HeaderValue::from(seed.value()));
    response
}
