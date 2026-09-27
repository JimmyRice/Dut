use crate::{
    application::{line_status::LineStatusService, next_train::NextTrainService},
    infrastructure::mtr::{line_status::MtrLineStatusSource, next_train::MtrNextTrainSource},
};

pub type LiveNextTrainService = NextTrainService<MtrNextTrainSource>;
pub type LiveLineStatusService = LineStatusService<MtrLineStatusSource>;

/// Dependencies shared by every handler. Cloning is cheap: services hold
/// their dependencies behind `Arc`.
#[derive(Clone, Debug)]
pub struct AppState {
    next_trains: LiveNextTrainService,
    line_status: LiveLineStatusService,
}

impl AppState {
    pub(crate) const fn new(
        next_trains: LiveNextTrainService,
        line_status: LiveLineStatusService,
    ) -> Self {
        Self {
            next_trains,
            line_status,
        }
    }

    pub(crate) const fn next_trains(&self) -> &LiveNextTrainService {
        &self.next_trains
    }

    pub(crate) const fn line_status(&self) -> &LiveLineStatusService {
        &self.line_status
    }
}
