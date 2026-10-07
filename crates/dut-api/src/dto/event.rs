//! Bodies of the events streamed to clients.
//!
//! Each kind of event has its own body, decoupled from the monitor's
//! internal changes: clients depend on what riders are told, not on how the
//! monitor happens to model its sources.

use jiff::Timestamp;
use serde::Serialize;

use dut_core::domain::line_status::{LineCondition, LineStatus, LineStatusChange};

use crate::dto::{
    common::{HktTime, LineRef},
    line_status::{condition_code, display_code},
};

/// `hello`: sent once when a stream opens, so a client knows it is connected
/// before anything changes.
#[derive(Debug, Serialize)]
pub(crate) struct HelloEvent {
    pub server_time: HktTime,
}

/// `line_status`: a line's condition or explanation changed.
#[derive(Debug, Serialize)]
pub(crate) struct LineStatusEvent<'a> {
    /// When this service noticed the change.
    pub observed_at: HktTime,
    pub line: LineRef,
    pub color: &'static str,
    pub previous: ConditionBody<'a>,
    pub current: ConditionBody<'a>,
}

impl<'a> LineStatusEvent<'a> {
    pub(crate) fn new(observed_at: Timestamp, change: &'a LineStatusChange) -> Self {
        Self {
            observed_at: HktTime(observed_at),
            line: change.current.line.into(),
            color: change.current.line.color(),
            previous: ConditionBody::from(&change.previous),
            current: ConditionBody::from(&change.current),
        }
    }
}

/// A line's condition on one side of a change, in the terms of
/// `GET /api/lines/status`.
#[derive(Debug, Serialize)]
pub(crate) struct ConditionBody<'a> {
    pub condition: &'static str,
    pub display: &'static str,
    pub message: Option<&'a str>,
}

impl<'a> From<&'a LineStatus> for ConditionBody<'a> {
    fn from(status: &'a LineStatus) -> Self {
        Self {
            condition: condition_code(&status.condition),
            display: display_code(LineCondition::display_color(&status.condition)),
            message: status.message.as_deref(),
        }
    }
}
