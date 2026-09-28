use std::error::Error;

use thiserror::Error;

use dut_core::domain::reference::SourceFile;
use dut_http::UpstreamError;

/// Why a poll of MTR open data produced nothing. Any of these fails the
/// whole poll, so the last complete set of files and datasets stays in use.
#[derive(Debug, Error)]
pub(super) enum OpenDataError {
    #[error(transparent)]
    Upstream(#[from] UpstreamError),

    #[error("{file} is not the CSV expected")]
    Csv {
        file: SourceFile,
        #[source]
        cause: csv::Error,
    },

    #[error("{file} has an invalid {column}: {value:?}")]
    InvalidValue {
        file: SourceFile,
        column: &'static str,
        value: String,
        #[source]
        cause: Box<dyn Error + Send + Sync>,
    },

    #[error("{file} has an unexpected {column}: {value:?}")]
    UnexpectedValue {
        file: SourceFile,
        column: &'static str,
        value: String,
    },
}
