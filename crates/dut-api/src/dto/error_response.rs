use std::borrow::Cow;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    pub error: ErrorDetail,
}

/// Fixed text for every error but one, so building and cloning a detail
/// allocates nothing unless it names the path that was not found.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ErrorDetail {
    pub code: &'static str,
    pub message: Cow<'static, str>,
}
