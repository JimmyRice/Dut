mod common;
mod error_response;
mod line_status;
mod lines;
mod next_train;

pub use common::{HktTime, LineRef, LocalizedText, StationRef};
pub use error_response::{ErrorDetail, ErrorResponse};
pub use line_status::{LineStatusBody, LineStatusResponse};
pub use lines::{Destinations, LineDetail, LinesResponse};
pub use next_train::{
    AlertBody, BoardBody, LineBoardBody, NextTrainResponse, NoticeBody, StationBoardsResponse,
    TrainBody, Upcoming,
};
