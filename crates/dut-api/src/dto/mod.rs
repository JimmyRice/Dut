mod common;
mod error_response;
mod line_status;
mod lines;
mod next_train;

pub(crate) use error_response::{ErrorDetail, ErrorResponse};
pub(crate) use line_status::LineStatusResponse;
pub(crate) use lines::LinesResponse;
pub(crate) use next_train::{NextTrainResponse, StationBoardsResponse};
