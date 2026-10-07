//! Parsing of path and query parameters into domain types.

use std::{ops::RangeInclusive, time::Duration};

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
    scenario_named(query.scenario.as_deref(), query.seed)
}

/// Resolves a scenario name and seed already read from a query string.
pub(super) fn scenario_named<S: Scenario>(
    name: Option<&str>,
    seed: Option<u64>,
) -> Result<(S, Seed), ApiError> {
    let choice = match name {
        Some(name) => name
            .parse::<ScenarioChoice<S>>()
            .map_err(|_| ApiError::UnknownScenario)?,
        None => ScenarioChoice::Random,
    };
    Ok(choice.resolve(seed.map(Seed::new)))
}

/// How long the mock event stream waits between changes: five seconds
/// unless asked, and never so fast or slow that a demo is unusable.
pub(super) fn event_interval(seconds: Option<u64>) -> Result<Duration, ApiError> {
    const DEFAULT: u64 = 5;
    const ALLOWED: RangeInclusive<u64> = 1..=60;
    let seconds = seconds.unwrap_or(DEFAULT);
    ALLOWED
        .contains(&seconds)
        .then(|| Duration::from_secs(seconds))
        .ok_or(ApiError::InvalidQuery)
}
