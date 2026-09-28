use serde::Serialize;

use dut_core::{
    application::source::Snapshot,
    domain::reference::{
        LightRailNetwork, LightRailRoute, ReferenceData, RouteNumber, Stop, StopId,
    },
};

use crate::dto::common::{DatasetMeta, LocalizedText};

/// `GET /api/data/light-rail`: every stop, and the stops each route calls
/// at in each direction.
#[derive(Debug, Serialize)]
pub(crate) struct LightRailResponse<'a> {
    #[serde(flatten)]
    pub meta: DatasetMeta,
    pub stops: Vec<StopBody<'a>>,
    pub routes: Vec<RouteBody<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StopBody<'a> {
    /// The number the MTR's Light Rail real-time API also uses.
    pub id: u16,
    pub code: &'a str,
    pub name: LocalizedText<'a>,
    /// Numbers of the routes that call here.
    pub routes: Vec<&'a str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RouteBody<'a> {
    pub route: &'a str,
    pub directions: Vec<RouteDirectionBody<'a>>,
}

/// One direction of a route, named by where it starts and ends.
#[derive(Debug, Serialize)]
pub(crate) struct RouteDirectionBody<'a> {
    pub from: Option<StopRef<'a>>,
    pub towards: Option<StopRef<'a>>,
    /// Numbers of the stops called at, in order.
    pub stops: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct StopRef<'a> {
    pub id: u16,
    pub name: Option<LocalizedText<'a>>,
}

impl<'a> From<&'a Snapshot<ReferenceData>> for LightRailResponse<'a> {
    fn from(snapshot: &'a Snapshot<ReferenceData>) -> Self {
        let dataset = &snapshot.value().light_rail;
        let network = dataset.value();
        Self {
            meta: DatasetMeta::new(snapshot, dataset),
            stops: network
                .stops
                .iter()
                .map(|stop| StopBody::new(network, stop))
                .collect(),
            routes: network
                .routes
                .iter()
                .map(|route| RouteBody::new(network, route))
                .collect(),
        }
    }
}

impl<'a> StopBody<'a> {
    fn new(network: &'a LightRailNetwork, stop: &'a Stop) -> Self {
        Self {
            id: stop.id.get(),
            code: stop.code.as_str(),
            name: (&stop.name).into(),
            routes: network
                .routes_serving(stop.id)
                .map(RouteNumber::as_str)
                .collect(),
        }
    }
}

impl<'a> RouteBody<'a> {
    fn new(network: &'a LightRailNetwork, route: &'a LightRailRoute) -> Self {
        let stop = |id: &StopId| StopRef {
            id: id.get(),
            name: network.stop(*id).map(|stop| (&stop.name).into()),
        };
        Self {
            route: route.number.as_str(),
            directions: route
                .directions
                .iter()
                .map(|stops| RouteDirectionBody {
                    from: stops.first().map(stop),
                    towards: stops.last().map(stop),
                    stops: stops.iter().map(|id| id.get()).collect(),
                })
                .collect(),
        }
    }
}
