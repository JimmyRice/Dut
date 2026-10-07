//! Whether the service can currently see each upstream it reads in the
//! background.

use crate::domain::string_enum::string_enum;

string_enum! {
    /// An upstream read in the background.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum SourceId {
        /// The MTR line status feed.
        MtrLineStatus => "mtr.line_status",
        /// Next Train boards, sampled at a few stations for delay flags and
        /// special arrangement notices.
        MtrNextTrain => "mtr.next_train",
        /// The Hong Kong Observatory's weather warnings.
        HkoWarnings => "hko.warnings",
        /// The files on the MTR's open data portal: stations, fares, Light Rail,
        /// and barrier-free facilities.
        MtrOpenData => "mtr.open_data",
    }

    /// Short, stable name used in logs, such as `mtr.line_status`.
    pub const fn code;
}

string_enum! {
    /// How well the service can currently see a source.
    ///
    /// A source that cannot be read says nothing about service: its last known
    /// data is kept, and only this state reports the outage.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum HealthState {
        /// No read has finished yet.
        Starting => "starting",
        /// The last read succeeded.
        Healthy => "healthy",
        /// Recent reads failed, but the last success is recent enough that its
        /// data can still be relied on.
        Failing => "failing",
        /// Nothing has been read for so long that the source's data can no longer
        /// be relied on.
        Blind => "blind",
    }

    /// Lowercase name used in logs, such as `blind`.
    pub const fn code;
}

/// A source moved from one [`HealthState`] to another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HealthChange {
    pub source: SourceId,
    pub previous: HealthState,
    pub current: HealthState,
}
