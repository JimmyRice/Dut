use serde::Serialize;

use crate::{
    api::dto::common::{DirectionCode, LineRef, StationRef},
    domain::network::{Direction, Line, StationCode},
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
    /// Where each direction heads, as platform signs name it.
    pub directions: Vec<LineDirection>,
}

/// One running direction of a line and the termini it heads towards.
#[derive(Debug, Serialize)]
pub struct LineDirection {
    pub direction: DirectionCode,
    pub towards: Vec<StationRef<'static>>,
}

impl From<Line> for LineDetail {
    fn from(line: Line) -> Self {
        Self {
            line: line.into(),
            color: line.color(),
            stations: station_refs(line.stations()),
            directions: Direction::ALL
                .into_iter()
                .map(|direction| LineDirection {
                    direction: DirectionCode(direction),
                    towards: station_refs(line.termini().get(direction)),
                })
                .collect(),
        }
    }
}

fn station_refs(codes: &'static [StationCode]) -> Vec<StationRef<'static>> {
    codes.iter().map(StationRef::from).collect()
}
