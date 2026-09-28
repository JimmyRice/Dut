use serde::Serialize;

use dut_core::{
    application::source::Snapshot,
    domain::{
        network::{Line, StationCode},
        reference::{PublishedNetwork, PublishedStation, ReferenceData, Route},
    },
};

use crate::dto::common::{DatasetMeta, DirectionCode, LineRef, LocalizedText};

/// `GET /api/data/stations`: the station list and each line's routes, as
/// MTR open data publishes them.
#[derive(Debug, Serialize)]
pub(crate) struct PublishedStationsResponse<'a> {
    #[serde(flatten)]
    pub meta: DatasetMeta,
    pub stations: Vec<PublishedStationBody<'a>>,
    pub lines: Vec<PublishedLineBody<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PublishedStationBody<'a> {
    pub code: &'a str,
    pub name: LocalizedText<'a>,
    /// Codes of the lines whose routes call here.
    pub lines: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PublishedLineBody<'a> {
    #[serde(flatten)]
    pub line: LineRef,
    pub routes: Vec<RouteBody<'a>>,
}

/// One way trains run: a direction, or a branch of it, with its ends named
/// so riders can tell the routes apart.
#[derive(Debug, Serialize)]
pub(crate) struct RouteBody<'a> {
    pub direction: DirectionCode,
    pub from: Option<PublishedStationRef<'a>>,
    pub towards: Option<PublishedStationRef<'a>>,
    /// Codes of the stations called at, in order.
    pub stations: Vec<&'a str>,
}

/// A station by code, with its name as open data publishes it.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct PublishedStationRef<'a> {
    pub code: &'a str,
    pub name: Option<LocalizedText<'a>>,
}

impl<'a> From<&'a Snapshot<ReferenceData>> for PublishedStationsResponse<'a> {
    fn from(snapshot: &'a Snapshot<ReferenceData>) -> Self {
        let dataset = &snapshot.value().stations;
        let network = dataset.value();
        Self {
            meta: DatasetMeta::new(snapshot, dataset),
            stations: network
                .stations
                .iter()
                .map(|station| PublishedStationBody::new(network, station))
                .collect(),
            lines: Line::ALL
                .into_iter()
                .filter_map(|line| PublishedLineBody::new(network, line))
                .collect(),
        }
    }
}

impl<'a> PublishedStationBody<'a> {
    fn new(network: &'a PublishedNetwork, station: &'a PublishedStation) -> Self {
        Self {
            code: station.code.as_str(),
            name: (&station.name).into(),
            lines: Line::ALL
                .into_iter()
                .filter(|line| {
                    network
                        .routes
                        .iter()
                        .any(|route| route.line == *line && route.stations.contains(&station.code))
                })
                .map(Line::code)
                .collect(),
        }
    }
}

impl<'a> PublishedLineBody<'a> {
    /// `None` for a line open data has no routes for, such as Light Rail.
    fn new(network: &'a PublishedNetwork, line: Line) -> Option<Self> {
        let routes: Vec<_> = network
            .routes
            .iter()
            .filter(|route| route.line == line)
            .map(|route| RouteBody::new(network, route))
            .collect();
        (!routes.is_empty()).then(|| Self {
            line: line.into(),
            routes,
        })
    }
}

impl<'a> RouteBody<'a> {
    fn new(network: &'a PublishedNetwork, route: &'a Route) -> Self {
        let station = |code: &'a StationCode| PublishedStationRef {
            code: code.as_str(),
            name: network.station(*code).map(|station| (&station.name).into()),
        };
        Self {
            direction: DirectionCode(route.direction),
            from: route.stations.first().map(station),
            towards: route.stations.last().map(station),
            stations: route.stations.iter().map(StationCode::as_str).collect(),
        }
    }
}
