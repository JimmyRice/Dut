//! Handlers, one module per group of endpoints. Each group supplies its own
//! router, and `crate::router` combines them.

pub(crate) mod data;
pub(crate) mod health;
pub(crate) mod line_status;
pub(crate) mod lines;
pub(crate) mod mock;
pub(crate) mod next_train;

mod params;
