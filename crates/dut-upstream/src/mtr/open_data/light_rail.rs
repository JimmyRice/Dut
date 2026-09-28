//! `light_rail_routes_and_stops.csv`: Light Rail stops, and the stops each
//! route calls at.

use std::collections::BTreeMap;

use serde::Deserialize;

use dut_core::domain::reference::{
    LightRailNetwork, LightRailRoute, RouteNumber, SourceFile, Stop, StopCode, StopId,
};

use crate::mtr::open_data::{
    decode::{clean_localized, decode_rows, parse},
    error::OpenDataError,
};

const FILE: SourceFile = SourceFile::LightRailRoutesAndStops;

#[derive(Debug, Deserialize)]
struct RouteStopRow {
    #[serde(rename = "Line Code")]
    route: String,
    /// `1` or `2`.
    #[serde(rename = "Direction")]
    direction: String,
    #[serde(rename = "Stop Code")]
    code: String,
    #[serde(rename = "Stop ID")]
    id: u16,
    #[serde(rename = "Chinese Name")]
    name_tc: String,
    #[serde(rename = "English Name")]
    name_en: String,
    #[serde(rename = "Sequence")]
    sequence: f64,
}

/// The file as a [`LightRailNetwork`]: stops and routes by number, and each
/// direction's stops in calling order.
///
/// Some routes list a stop twice in a row where trains loop at a terminus;
/// riders see one stop, so the repeat is dropped.
pub(super) fn clean(body: &[u8]) -> Result<LightRailNetwork, OpenDataError> {
    let mut stops = BTreeMap::new();
    let mut routes: BTreeMap<RouteNumber, BTreeMap<String, Vec<(f64, StopId)>>> = BTreeMap::new();

    for row in decode_rows::<RouteStopRow>(FILE, body)? {
        let id = StopId::new(row.id);
        let code: StopCode = parse(FILE, "Stop Code", &row.code)?;
        let number: RouteNumber = parse(FILE, "Line Code", &row.route)?;
        stops.entry(id).or_insert_with(|| Stop {
            id,
            code,
            name: clean_localized(&row.name_en, &row.name_tc),
        });

        routes
            .entry(number)
            .or_default()
            .entry(row.direction)
            .or_default()
            .push((row.sequence, id));
    }

    Ok(LightRailNetwork {
        stops: stops.into_values().collect(),
        routes: routes
            .into_iter()
            .map(|(number, directions)| LightRailRoute {
                number,
                directions: directions.into_values().map(calling_order).collect(),
            })
            .collect(),
    })
}

fn calling_order(mut stops: Vec<(f64, StopId)>) -> Vec<StopId> {
    stops.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut ids: Vec<_> = stops.into_iter().map(|(_, id)| id).collect();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn network() -> LightRailNetwork {
        clean(&fixtures::read(
            "mtr/open_data/light_rail_routes_and_stops.csv",
        ))
        .expect("the captured Light Rail network should clean")
    }

    fn stops(ids: &[u16]) -> Vec<StopId> {
        ids.iter().copied().map(StopId::new).collect()
    }

    #[test]
    fn lists_every_stop_once_by_number() {
        let network = network();

        assert_eq!(network.stops.len(), 68);
        let ferry_pier = network.stop(StopId::new(1)).expect("stop 1 is listed");
        assert_eq!(ferry_pier.name.en, "Tuen Mun Ferry Pier");
        assert_eq!(ferry_pier.name.tc, "屯門碼頭");
    }

    #[test]
    fn orders_routes_by_number() {
        let numbers: Vec<_> = network()
            .routes
            .iter()
            .map(|route| route.number.as_str().to_owned())
            .collect();

        assert_eq!(
            numbers,
            [
                "505", "507", "610", "614", "614P", "615", "615P", "705", "706", "751", "761P"
            ]
        );
    }

    #[test]
    fn drops_a_stop_listed_twice_in_a_row() {
        let network = network();
        let route_751 = network
            .routes
            .iter()
            .find(|route| route.number.as_str() == "751")
            .expect("route 751 is listed");

        let towards_tin_yat = &route_751.directions[0];
        assert_eq!(towards_tin_yat.len(), 23);
        assert_eq!(towards_tin_yat.last(), Some(&StopId::new(550)));
        assert_ne!(towards_tin_yat[21], StopId::new(550));
    }

    #[test]
    fn orders_each_direction_by_sequence() {
        let network = network();
        let route_705 = network
            .routes
            .iter()
            .find(|route| route.number.as_str() == "705")
            .expect("route 705 is listed");

        assert_eq!(route_705.directions[0], stops(&[430, 435, 450, 455, 500]));
    }
}
