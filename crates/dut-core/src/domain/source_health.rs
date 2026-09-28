//! Whether the service can currently see each upstream it reads in the
//! background.

use std::fmt;

/// An upstream read in the background.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceId {
    /// The MTR line status feed.
    MtrLineStatus,
    /// Next Train boards, sampled at a few stations for delay flags and
    /// special arrangement notices.
    MtrNextTrain,
    /// The Hong Kong Observatory's weather warnings.
    HkoWarnings,
}

impl SourceId {
    /// Short, stable name used in logs, such as `mtr.line_status`.
    pub const fn code(self) -> &'static str {
        match self {
            Self::MtrLineStatus => "mtr.line_status",
            Self::MtrNextTrain => "mtr.next_train",
            Self::HkoWarnings => "hko.warnings",
        }
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// How well the service can currently see a source.
///
/// A source that cannot be read says nothing about service: its last known
/// data is kept, and only this state reports the outage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthState {
    /// No read has finished yet.
    Starting,
    /// The last read succeeded.
    Healthy,
    /// Recent reads failed, but the last success is recent enough that its
    /// data can still be relied on.
    Failing,
    /// Nothing has been read for so long that the source's data can no longer
    /// be relied on.
    Blind,
}

impl HealthState {
    /// Lowercase name used in logs, such as `blind`.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Failing => "failing",
            Self::Blind => "blind",
        }
    }
}

impl fmt::Display for HealthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// A source moved from one [`HealthState`] to another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HealthChange {
    pub source: SourceId,
    pub previous: HealthState,
    pub current: HealthState,
}
