//! `mtr_lines_and_stations.csv`: the heavy rail stations, and the order
//! each line's trains call at them.

use std::collections::BTreeMap;

use serde::Deserialize;

use dut_core::domain::{
    network::{Direction, Line, StationCode},
    reference::{PublishedNetwork, PublishedStation, Route, SourceFile},
};

use crate::mtr::open_data::{
    decode::{clean_localized, decode_rows, parse},
    error::OpenDataError,
    skipped::Skipped,
    station_index::StationIndex,
};

const FILE: SourceFile = SourceFile::LinesAndStations;

#[derive(Debug, Deserialize)]
struct StationRow {
    #[serde(rename = "Line Code")]
    line: String,
    /// `UT` or `DT`, with a prefix such as `LMC-` for a branch.
    #[serde(rename = "Direction")]
    direction: String,
    #[serde(rename = "Station Code")]
    station: String,
    #[serde(rename = "Station ID")]
    id: u16,
    #[serde(rename = "Chinese Name")]
    name_tc: String,
    #[serde(rename = "English Name")]
    name_en: String,
    /// Published with decimals, such as `1.00`.
    #[serde(rename = "Sequence")]
    sequence: f64,
}

/// Orders routes by line, then up before down, then the main line before
/// its branches.
#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RouteKey {
    line: Line,
    down: bool,
    branch: String,
}

/// The station list as a [`PublishedNetwork`], and the index from the
/// numeric IDs other files use to station codes.
///
/// A row on a line this service does not know still contributes its
/// station, so fares and facilities for a new line's stations resolve, and
/// the station shows up as drift.
pub(super) fn clean(body: &[u8]) -> Result<(PublishedNetwork, StationIndex), OpenDataError> {
    let rows: Vec<StationRow> = decode_rows(FILE, body)?;
    let mut unknown_lines = Skipped::new(FILE, "unknown line code");
    let mut stations = BTreeMap::new();
    let mut index = StationIndex::default();
    let mut routes: BTreeMap<RouteKey, Vec<(f64, StationCode)>> = BTreeMap::new();

    for row in rows {
        let code: StationCode = parse(FILE, "Station Code", &row.station)?;
        stations
            .entry(code)
            .or_insert_with(|| clean_localized(&row.name_en, &row.name_tc));
        index.insert(row.id, code);

        let (branch, direction) = direction(&row.direction)?;
        let Ok(line) = row.line.parse::<Line>() else {
            unknown_lines.record(&row.line);
            continue;
        };
        let key = RouteKey {
            line,
            down: direction == Direction::Down,
            branch: branch.to_owned(),
        };
        routes.entry(key).or_default().push((row.sequence, code));
    }
    unknown_lines.report();

    let network = PublishedNetwork {
        stations: stations
            .into_iter()
            .map(|(code, name)| PublishedStation { code, name })
            .collect(),
        routes: routes.into_iter().map(route).collect(),
    };
    Ok((network, index))
}

/// Splits a direction such as `LMC-UT` into its branch and direction.
fn direction(value: &str) -> Result<(&str, Direction), OpenDataError> {
    let (branch, direction) = value.rsplit_once('-').unwrap_or(("", value));
    match direction {
        "UT" => Ok((branch, Direction::Up)),
        "DT" => Ok((branch, Direction::Down)),
        _ => Err(OpenDataError::UnexpectedValue {
            file: FILE,
            column: "Direction",
            value: value.to_owned(),
        }),
    }
}

fn route((key, mut stops): (RouteKey, Vec<(f64, StationCode)>)) -> Route {
    stops.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut stations: Vec<_> = stops.into_iter().map(|(_, code)| code).collect();
    stations.dedup();
    Route {
        line: key.line,
        direction: if key.down {
            Direction::Down
        } else {
            Direction::Up
        },
        stations,
    }
}

#[cfg(test)]
mod tests {
    use dut_core::domain::localized::Localized;

    use super::*;
    use crate::fixtures;

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    fn codes(codes: &[&str]) -> Vec<StationCode> {
        codes.iter().map(|value| code(value)).collect()
    }

    fn published() -> (PublishedNetwork, StationIndex) {
        clean(&fixtures::read("mtr/open_data/mtr_lines_and_stations.csv"))
            .expect("the captured station list should clean")
    }

    fn routes(network: &PublishedNetwork, line: Line) -> Vec<(Direction, Vec<StationCode>)> {
        network
            .routes
            .iter()
            .filter(|route| route.line == line)
            .map(|route| (route.direction, route.stations.clone()))
            .collect()
    }

    #[test]
    fn lists_every_station_once_by_code() {
        let (network, _) = published();

        assert_eq!(network.stations.len(), 97);
        assert!(
            network
                .stations
                .windows(2)
                .all(|pair| pair[0].code < pair[1].code)
        );
    }

    #[test]
    fn uses_the_standard_form_of_lai() {
        let (network, _) = published();
        let lai_king = network.stations.iter().find(|s| s.code == code("LAK"));

        assert_eq!(
            lai_king.map(|station| &station.name),
            Some(&Localized::new("Lai King".to_owned(), "荔景".to_owned()))
        );
    }

    #[test]
    fn keeps_branches_as_their_own_routes() {
        let (network, _) = published();

        assert_eq!(
            routes(&network, Line::TseungKwanO),
            vec![
                (
                    Direction::Up,
                    codes(&["NOP", "QUB", "YAT", "TIK", "TKO", "HAH", "POA"])
                ),
                (Direction::Up, codes(&["TIK", "TKO", "LHP"])),
                (
                    Direction::Down,
                    codes(&["POA", "HAH", "TKO", "TIK", "YAT", "QUB", "NOP"])
                ),
                (Direction::Down, codes(&["LHP", "TKO", "TIK"])),
            ]
        );
    }

    #[test]
    fn follows_the_published_direction_of_each_line() {
        let (network, _) = published();

        assert_eq!(
            routes(&network, Line::DisneylandResort),
            vec![
                (Direction::Up, codes(&["DIS", "SUN"])),
                (Direction::Down, codes(&["SUN", "DIS"])),
            ]
        );
    }

    #[test]
    fn indexes_both_ids_of_shared_stations() {
        let (_, index) = published();

        assert_eq!(index.get(39), Some(code("HOK")));
        assert_eq!(index.get(44), Some(code("HOK")));
        assert_eq!(index.get(70), None);
    }

    #[test]
    fn agrees_with_the_compiled_network_apart_from_racecourse() {
        use dut_core::domain::reference::NetworkDrift;

        let (network, _) = published();

        assert_eq!(
            network.drift(),
            vec![NetworkDrift::Unpublished(code("RAC"))]
        );
    }

    #[test]
    fn rejects_an_unknown_direction() {
        let body = "Line Code,Direction,Station Code,Station ID,Chinese Name,English Name,Sequence\n\
                    TKL,XT,TKO,50,將軍澳,Tseung Kwan O,1.00\n";

        assert!(matches!(
            clean(body.as_bytes()),
            Err(OpenDataError::UnexpectedValue { .. })
        ));
    }
}
