mod line_status;
mod lines;
mod next_train;
mod params;

use axum::Router;

use crate::state::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .merge(lines::router())
        .merge(line_status::router())
        .merge(next_train::router())
}
