use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

use dut_core::application::{next_train::NextTrainError, source::SourceUnavailable};

use crate::dto::{ErrorDetail, ErrorResponse};

/// Every error an API client can see. Messages never echo client input or
/// upstream details.
#[derive(Debug, Error)]
pub(crate) enum ApiError {
    #[error("route not found: {path}")]
    NotFound { path: String },

    #[error("unknown line")]
    UnknownLine,

    #[error("unknown station")]
    UnknownStation,

    #[error("the line does not serve the station")]
    StationNotOnLine,

    #[error("an upstream service is unavailable")]
    UpstreamUnavailable,
}

impl ApiError {
    pub(crate) fn not_found(path: impl Into<String>) -> Self {
        Self::NotFound { path: path.into() }
    }

    /// The error as it appears in response bodies.
    pub(crate) fn detail(&self) -> ErrorDetail {
        ErrorDetail {
            code: self.code().to_owned(),
            message: self.client_message(),
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound { .. }
            | Self::UnknownLine
            | Self::UnknownStation
            | Self::StationNotOnLine => StatusCode::NOT_FOUND,
            Self::UpstreamUnavailable => StatusCode::BAD_GATEWAY,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::UnknownLine => "unknown_line",
            Self::UnknownStation => "unknown_station",
            Self::StationNotOnLine => "station_not_on_line",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }

    fn client_message(&self) -> String {
        match self {
            Self::NotFound { path } => format!("No route matches {path}"),
            Self::UnknownLine => "No line matches the requested line code".to_owned(),
            Self::UnknownStation => "No station matches the requested station code".to_owned(),
            Self::StationNotOnLine => {
                "The requested line does not serve the requested station".to_owned()
            }
            Self::UpstreamUnavailable => "An upstream service is unavailable".to_owned(),
        }
    }
}

impl From<NextTrainError> for ApiError {
    fn from(error: NextTrainError) -> Self {
        match error {
            NextTrainError::UnknownStation(_) => Self::UnknownStation,
            NextTrainError::StationNotOnLine { .. } => Self::StationNotOnLine,
            NextTrainError::Unavailable(_) => Self::UpstreamUnavailable,
        }
    }
}

impl From<SourceUnavailable> for ApiError {
    fn from(_: SourceUnavailable) -> Self {
        Self::UpstreamUnavailable
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse {
            error: self.detail(),
        };

        (status, Json(body)).into_response()
    }
}
