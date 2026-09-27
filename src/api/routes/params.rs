//! Parsing of path parameters into domain types.

use crate::{
    api::error::ApiError,
    domain::network::{Line, StationCode},
};

/// Parses a line code case-insensitively.
pub(super) fn line(code: &str) -> Result<Line, ApiError> {
    code.parse().map_err(|_| ApiError::UnknownLine)
}

/// Parses a station code case-insensitively. Whether the station exists is
/// decided by the application layer.
pub(super) fn station(code: &str) -> Result<StationCode, ApiError> {
    code.parse().map_err(|_| ApiError::UnknownStation)
}
