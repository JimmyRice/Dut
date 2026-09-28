mod line_status;
mod lines;
mod next_train;
mod params;

use axum::Router;
use dut_core::application::{line_status::LineStatusSource, next_train::NextTrainSource};

use crate::state::AppState;

pub(super) fn router<N: NextTrainSource, L: LineStatusSource>() -> Router<AppState<N, L>> {
    Router::new()
        .merge(lines::router())
        .merge(line_status::router())
        .merge(next_train::router())
}
