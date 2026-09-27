use serde::Serialize;

use crate::{
    api::dto::common::{HktTime, LineRef},
    application::source::Snapshot,
    domain::line_status::{DisplayColor, LineCondition, LineStatus, NetworkStatus},
};

/// `GET /api/lines/status`
#[derive(Debug, Serialize)]
pub struct LineStatusResponse<'a> {
    /// When the MTR last published the feed.
    pub updated_at: HktTime,
    /// When this service fetched it.
    pub fetched_at: HktTime,
    pub stale: bool,
    pub lines: Vec<LineStatusBody<'a>>,
}

impl<'a> From<&'a Snapshot<NetworkStatus>> for LineStatusResponse<'a> {
    fn from(snapshot: &'a Snapshot<NetworkStatus>) -> Self {
        let status = snapshot.value();
        Self {
            updated_at: HktTime(status.updated_at),
            fetched_at: HktTime(snapshot.fetched_at()),
            stale: snapshot.freshness().is_stale(),
            lines: status.lines.iter().map(LineStatusBody::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LineStatusBody<'a> {
    pub line: LineRef,
    pub color: &'static str,
    /// What the MTR reports, e.g. `delayed_or_disrupted`.
    pub condition: &'static str,
    /// How the MTR website shows it: `green`, `yellow`, `red`, `grey`, or
    /// `typhoon`.
    pub display: &'static str,
    pub message: Option<&'a str>,
}

impl<'a> From<&'a LineStatus> for LineStatusBody<'a> {
    fn from(status: &'a LineStatus) -> Self {
        Self {
            line: status.line.into(),
            color: status.line.color(),
            condition: condition_code(&status.condition),
            display: display_code(status.condition.display_color()),
            message: status.message.as_deref(),
        }
    }
}

const fn condition_code(condition: &LineCondition) -> &'static str {
    match condition {
        LineCondition::Normal => "normal",
        LineCondition::Delayed => "delayed",
        LineCondition::Disrupted => "disrupted",
        LineCondition::DelayedOrDisrupted => "delayed_or_disrupted",
        LineCondition::NonServiceHours => "non_service_hours",
        LineCondition::TyphoonSignal => "typhoon_signal",
        LineCondition::Unknown(_) => "unknown",
    }
}

const fn display_code(color: DisplayColor) -> &'static str {
    match color {
        DisplayColor::Green => "green",
        DisplayColor::Yellow => "yellow",
        DisplayColor::Red => "red",
        DisplayColor::Grey => "grey",
        DisplayColor::Typhoon => "typhoon",
    }
}
