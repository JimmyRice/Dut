//! Parsing of path and query parameters into domain types.

use axum::extract::{Query, rejection::QueryRejection};
use dut_core::domain::network::{Line, StationCode};
use dut_mock::{Scenario, ScenarioChoice, Seed};

use crate::{dto::MockQuery, error::ApiError};

/// Parses a line code case-insensitively.
pub(super) fn line(code: &str) -> Result<Line, ApiError> {
    code.parse().map_err(|_| ApiError::UnknownLine)
}

/// Parses a station code case-insensitively. Whether the station exists is
/// decided by the application layer.
pub(super) fn station(code: &str) -> Result<StationCode, ApiError> {
    code.parse().map_err(|_| ApiError::UnknownStation)
}

/// Reads which scenario a mock route should simulate, and in which world.
/// An absent scenario is `random`.
pub(super) fn scenario<S: Scenario>(
    query: Result<Query<MockQuery>, QueryRejection>,
) -> Result<(S, Seed), ApiError> {
    let Query(query) = query.map_err(|_| ApiError::InvalidQuery)?;
    let choice = match query.scenario.as_deref() {
        Some(name) => name
            .parse::<ScenarioChoice<S>>()
            .map_err(|_| ApiError::UnknownScenario)?,
        None => ScenarioChoice::Random,
    };
    Ok(choice.resolve(query.seed.map(Seed::new)))
}
