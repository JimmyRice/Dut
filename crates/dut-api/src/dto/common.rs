//! Building blocks shared by response bodies.
//!
//! Response DTOs borrow from cached domain data instead of copying it, so
//! serving a cached value allocates little beyond the JSON output itself.

use jiff::Timestamp;
use serde::{Serialize, Serializer};

use dut_core::domain::{
    localized::Localized,
    network::{Direction, Line, Station, StationCode},
    time::HONG_KONG,
};

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct LocalizedText<'a> {
    pub en: &'a str,
    pub tc: &'a str,
}

impl<'a> From<Localized<&'a str>> for LocalizedText<'a> {
    fn from(text: Localized<&'a str>) -> Self {
        Self {
            en: text.en,
            tc: text.tc,
        }
    }
}

/// A line identified by code, with its display name.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct LineRef {
    pub code: &'static str,
    pub name: LocalizedText<'static>,
}

impl From<Line> for LineRef {
    fn from(line: Line) -> Self {
        Self {
            code: line.code(),
            name: line.name().into(),
        }
    }
}

/// A station identified by code, with its display name when the station is
/// known to this service.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct StationRef<'a> {
    pub code: &'a str,
    pub name: Option<LocalizedText<'static>>,
}

impl<'a> From<&'a StationCode> for StationRef<'a> {
    fn from(code: &'a StationCode) -> Self {
        Self {
            code: code.as_str(),
            name: Station::find(*code).map(|station| station.name.into()),
        }
    }
}

/// A running direction as its stable identifier, `up` or `down`.
///
/// The identifiers match the MTR's so clients can remember a rider's usual
/// direction; the accompanying `towards` is what riders should be shown.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DirectionCode(pub Direction);

impl Serialize for DirectionCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self.0 {
            Direction::Up => "up",
            Direction::Down => "down",
        })
    }
}

/// A timestamp serialized as RFC 3339 in Hong Kong Time with whole seconds,
/// e.g. `2026-09-27T22:36:36+08:00`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HktTime(pub Timestamp);

impl Serialize for HktTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let whole_seconds = Timestamp::from_second(self.0.as_second()).unwrap_or(self.0);
        serializer.collect_str(&whole_seconds.display_with_offset(HONG_KONG))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_rendered_in_hong_kong_time_without_fractions() {
        let timestamp: Timestamp = "2026-09-27T14:36:36.789Z".parse().expect("valid timestamp");

        let json = serde_json::to_string(&HktTime(timestamp)).expect("time should serialize");

        assert_eq!(json, r#""2026-09-27T22:36:36+08:00""#);
    }

    #[test]
    fn unknown_stations_have_no_name() {
        let code: StationCode = "XYZ".parse().expect("valid code");

        let json = serde_json::to_value(StationRef::from(&code)).expect("ref should serialize");

        assert_eq!(json, serde_json::json!({ "code": "XYZ", "name": null }));
    }
}
