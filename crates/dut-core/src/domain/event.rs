//! What the monitor tells its subscribers.
//!
//! Events are facts, not judgements: every change in the sources is
//! published, including routine ones such as service ending for the night,
//! and nothing is ranked by severity. Each subscriber decides what matters to
//! its own audience.

use jiff::Timestamp;

use crate::domain::{
    line_status::LineStatusChange, next_train::SignalChange, source_health::HealthChange,
    weather::WarningChange,
};

/// One change the monitor observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorEvent {
    /// When the monitor noticed the change, which is when the poll that
    /// revealed it finished.
    pub observed_at: Timestamp,
    pub change: Change,
}

/// What changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    LineStatus(LineStatusChange),
    WeatherWarning(WarningChange),
    NextTrainSignal(SignalChange),
    /// A source became readable or unreadable. Its data does not change while
    /// it cannot be read, so an outage never looks like service resuming.
    SourceHealth(HealthChange),
}
