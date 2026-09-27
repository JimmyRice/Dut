use serde::Serialize;

use crate::{
    api::dto::common::{LineRef, StationRef},
    domain::network::{ByDirection, Line, StationCode},
};

/// `GET /api/lines`: the lines covered by Next Train, with their stations.
#[derive(Debug, Serialize)]
pub struct LinesResponse {
    pub lines: Vec<LineDetail>,
}

impl LinesResponse {
    /// Describes the static network compiled into this service.
    pub fn from_network() -> Self {
        Self {
            lines: Line::with_next_train().map(LineDetail::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LineDetail {
    #[serde(flatten)]
    pub line: LineRef,
    /// `#RRGGBB`.
    pub color: &'static str,
    /// In the order the line runs.
    pub stations: Vec<StationRef<'static>>,
    /// Where `up` and `down` trains terminate.
    pub destinations: Destinations,
}

#[derive(Debug, Serialize)]
pub struct Destinations {
    pub up: Vec<StationRef<'static>>,
    pub down: Vec<StationRef<'static>>,
}

impl From<Line> for LineDetail {
    fn from(line: Line) -> Self {
        let ByDirection { up, down } = line.destinations();
        Self {
            line: line.into(),
            color: line.color(),
            stations: station_refs(line.stations()),
            destinations: Destinations {
                up: station_refs(up),
                down: station_refs(down),
            },
        }
    }
}

fn station_refs(codes: &'static [StationCode]) -> Vec<StationRef<'static>> {
    codes.iter().map(StationRef::from).collect()
}
