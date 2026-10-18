use std::{fmt, sync::Arc};

use dut_core::application::{
    line_status::LineStatusService, next_train::NextTrainService,
    reference_data::ReferenceDataService,
};

use crate::{
    body_version::FetchVersion,
    encoded_body::EncodedBody,
    routes::{data::OpenDataBodies, next_train::NextTrainBodies},
};

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
    next_train_bodies: Arc<NextTrainBodies>,
    line_status_body: Arc<EncodedBody<FetchVersion>>,
    mock_api: bool,
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
            next_train_bodies: Arc::default(),
            line_status_body: Arc::new(EncodedBody::new("line-status")),
            mock_api: false,
        }
    }

    /// Also serves simulated data under `/api/mock` when `enabled`. Riders'
    /// apps never call it, so it is off unless the operator turns it on.
    #[must_use]
    pub const fn with_mock_api(mut self, enabled: bool) -> Self {
        self.mock_api = enabled;
        self
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

    pub(crate) fn next_train_bodies(&self) -> &NextTrainBodies {
        &self.next_train_bodies
    }

    pub(crate) fn line_status_body(&self) -> &EncodedBody<FetchVersion> {
        &self.line_status_body
    }

    pub(crate) const fn mock_api(&self) -> bool {
        self.mock_api
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
            next_train_bodies: Arc::clone(&self.next_train_bodies),
            line_status_body: Arc::clone(&self.line_status_body),
            mock_api: self.mock_api,
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
            .field("next_train_bodies", &self.next_train_bodies)
            .field("line_status_body", &self.line_status_body)
            .field("mock_api", &self.mock_api)
            .finish()
    }
}
