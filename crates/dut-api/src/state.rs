use std::{fmt, sync::Arc};

use dut_core::application::{
    line_status::LineStatusService, next_train::NextTrainService,
    reference_data::ReferenceDataService,
};

use crate::routes::data::OpenDataBodies;

/// Dependencies shared by every handler. Cloning is cheap: services hold
/// their dependencies behind `Arc`.
///
/// The data sources are type parameters rather than named adapters, so this
/// crate does not depend on `dut-upstream`; the binary picks them when it
/// builds the state.
pub struct AppState<N, L, R> {
    next_trains: NextTrainService<N>,
    line_status: LineStatusService<L>,
    reference_data: ReferenceDataService<R>,
    open_data_bodies: Arc<OpenDataBodies>,
}

impl<N, L, R> AppState<N, L, R> {
    pub fn new(
        next_trains: NextTrainService<N>,
        line_status: LineStatusService<L>,
        reference_data: ReferenceDataService<R>,
    ) -> Self {
        Self {
            next_trains,
            line_status,
            reference_data,
            open_data_bodies: Arc::default(),
        }
    }

    pub(crate) const fn next_trains(&self) -> &NextTrainService<N> {
        &self.next_trains
    }

    pub(crate) const fn line_status(&self) -> &LineStatusService<L> {
        &self.line_status
    }

    pub(crate) const fn reference_data(&self) -> &ReferenceDataService<R> {
        &self.reference_data
    }

    pub(crate) fn open_data_bodies(&self) -> &OpenDataBodies {
        &self.open_data_bodies
    }
}

// Implemented by hand so that none requires the same of `N`, `L`, or `R`.
impl<N, L, R> Clone for AppState<N, L, R> {
    fn clone(&self) -> Self {
        Self {
            next_trains: self.next_trains.clone(),
            line_status: self.line_status.clone(),
            reference_data: self.reference_data.clone(),
            open_data_bodies: Arc::clone(&self.open_data_bodies),
        }
    }
}

impl<N, L, R> fmt::Debug for AppState<N, L, R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("next_trains", &self.next_trains)
            .field("line_status", &self.line_status)
            .field("reference_data", &self.reference_data)
            .field("open_data_bodies", &self.open_data_bodies)
            .finish()
    }
}
