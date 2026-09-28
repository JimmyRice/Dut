use std::fmt;

use dut_core::application::{line_status::LineStatusService, next_train::NextTrainService};

/// Dependencies shared by every handler. Cloning is cheap: services hold
/// their dependencies behind `Arc`.
///
/// The data sources are type parameters rather than named adapters, so this
/// crate does not depend on `dut-upstream`; the binary picks them when it
/// builds the state.
pub struct AppState<N, L> {
    next_trains: NextTrainService<N>,
    line_status: LineStatusService<L>,
}

impl<N, L> AppState<N, L> {
    pub const fn new(next_trains: NextTrainService<N>, line_status: LineStatusService<L>) -> Self {
        Self {
            next_trains,
            line_status,
        }
    }

    pub(crate) const fn next_trains(&self) -> &NextTrainService<N> {
        &self.next_trains
    }

    pub(crate) const fn line_status(&self) -> &LineStatusService<L> {
        &self.line_status
    }
}

// Implemented by hand so that neither requires the same of `N` or `L`.
impl<N, L> Clone for AppState<N, L> {
    fn clone(&self) -> Self {
        Self {
            next_trains: self.next_trains.clone(),
            line_status: self.line_status.clone(),
        }
    }
}

impl<N, L> fmt::Debug for AppState<N, L> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("next_trains", &self.next_trains)
            .field("line_status", &self.line_status)
            .finish()
    }
}
