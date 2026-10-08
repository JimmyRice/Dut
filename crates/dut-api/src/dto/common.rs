//! Building blocks shared by response bodies.
//!
//! Response DTOs borrow from cached domain data instead of copying it, so
//! serving a cached value allocates little beyond the JSON output itself.

use jiff::Timestamp;
use serde::{Serialize, Serializer};

use dut_core::{
    application::source::Snapshot,
    domain::{
        localized::Localized,
        network::{Direction, Line, Station, StationCode},
        reference::{Dataset, ReferenceData},
        time::HONG_KONG,
    },
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

impl<'a> From<&'a Localized<String>> for LocalizedText<'a> {
    fn from(text: &'a Localized<String>) -> Self {
        Self {
            en: &text.en,
            tc: &text.tc,
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

/// When a dataset was published and fetched, which every dataset response
/// opens with.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct DatasetMeta {
    /// When the MTR last changed the files behind the dataset; `null` when
    /// the portal did not say.
    pub updated_at: Option<HktTime>,
    /// When this service fetched them.
    pub fetched_at: HktTime,
    pub stale: bool,
}

impl DatasetMeta {
    pub(crate) fn new<T>(snapshot: &Snapshot<ReferenceData>, dataset: &Dataset<T>) -> Self {
        Self {
            updated_at: dataset.updated_at().map(HktTime),
            fetched_at: HktTime(snapshot.fetched_at()),
            stale: snapshot.freshness().is_stale(),
        }
    }
}

/// Serializes a borrowed slice as a JSON array, converting each item as it
/// is written instead of collecting the converted items first. The MTR fare
/// table has over nine thousand trips.
pub(crate) struct MappedSeq<'a, T, M> {
    items: &'a [T],
    map: M,
}

impl<'a, T, M> MappedSeq<'a, T, M> {
    pub(crate) const fn new(items: &'a [T], map: M) -> Self {
        Self { items, map }
    }
}

impl<'a, T, M, B> Serialize for MappedSeq<'a, T, M>
where
    M: Fn(&'a T) -> B,
    B: Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.items.iter().map(&self.map))
    }
}

#[cfg(test)]
mod tests {
    use dut_core::station;

    use super::*;

    #[test]
    fn times_are_rendered_in_hong_kong_time_without_fractions() {
        let timestamp: Timestamp = "2026-09-27T14:36:36.789Z".parse().expect("valid timestamp");

        let json = serde_json::to_string(&HktTime(timestamp)).expect("time should serialize");

        assert_eq!(json, r#""2026-09-27T22:36:36+08:00""#);
    }

    #[test]
    fn mapped_sequences_convert_each_item() {
        let json = serde_json::to_string(&MappedSeq::new(&[1, 2, 3], |n: &i32| n * 10))
            .expect("sequence should serialize");

        assert_eq!(json, "[10,20,30]");
    }

    #[test]
    fn unknown_stations_have_no_name() {
        let code = station!("XYZ");

        let json = serde_json::to_value(StationRef::from(&code)).expect("ref should serialize");

        assert_eq!(json, serde_json::json!({ "code": "XYZ", "name": null }));
    }
}
