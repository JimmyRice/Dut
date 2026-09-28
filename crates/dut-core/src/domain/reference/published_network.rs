use std::collections::BTreeSet;

use crate::domain::{
    localized::Localized,
    network::{Direction, Line, Station, StationCode},
};

/// The heavy rail network as MTR open data lists it: each station with its
/// name, and the order in which each line's trains call at them.
///
/// It is served as published. The network compiled into this service
/// ([`Line`], [`Station`]) stays the one Next Train relies on; [`drift`]
/// reports where the two disagree, such as when a new station opens.
///
/// [`drift`]: Self::drift
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PublishedNetwork {
    /// Ordered by code.
    pub stations: Vec<PublishedStation>,
    /// Grouped by line, in the order the MTR lists them.
    pub routes: Vec<Route>,
}

/// A station as open data names it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PublishedStation {
    pub code: StationCode,
    pub name: Localized<String>,
}

/// The stations a line's trains call at, in order, running one way.
///
/// A line has a route for each direction, and another for each branch,
/// such as the East Rail Line's up trains to Lok Ma Chau. A branch's route
/// may cover only the branch itself: the Tseung Kwan O Line's to LOHAS
/// Park starts at Tiu Keng Leng.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Route {
    pub line: Line,
    pub direction: Direction,
    /// Never empty.
    pub stations: Vec<StationCode>,
}

/// A way the published network differs from the one compiled into this
/// service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkDrift {
    /// Open data lists a station this service does not know.
    UnknownStation(StationCode),
    /// Open data names a known station differently.
    Renamed {
        station: StationCode,
        published: Localized<String>,
    },
    /// Open data lists stations on a line that the compiled line lacks, or
    /// lacks some the compiled line has. Stations that open data leaves out
    /// everywhere are reported as [`Unpublished`](Self::Unpublished) instead.
    LineStations {
        line: Line,
        added: Vec<StationCode>,
        removed: Vec<StationCode>,
    },
    /// This service knows a station that open data leaves out altogether.
    /// Expected for Racecourse, which opens only on race days, so it is not a
    /// problem by itself.
    Unpublished(StationCode),
}

impl PublishedNetwork {
    /// Where this network differs from the compiled one, stations first,
    /// then lines in [`Line::ALL`] order.
    pub fn drift(&self) -> Vec<NetworkDrift> {
        let mut drift: Vec<_> = self.station_drift().collect();
        drift.extend(self.unpublished().map(NetworkDrift::Unpublished));
        drift.extend(
            Line::ALL
                .into_iter()
                .filter_map(|line| self.line_drift(line)),
        );
        drift
    }

    fn station(&self, code: StationCode) -> Option<&PublishedStation> {
        self.stations
            .binary_search_by_key(&code, |station| station.code)
            .ok()
            .and_then(|index| self.stations.get(index))
    }

    fn station_drift(&self) -> impl Iterator<Item = NetworkDrift> {
        self.stations.iter().filter_map(|published| {
            let Some(known) = Station::find(published.code) else {
                return Some(NetworkDrift::UnknownStation(published.code));
            };
            let renamed = known.name.en != published.name.en || known.name.tc != published.name.tc;
            renamed.then(|| NetworkDrift::Renamed {
                station: published.code,
                published: published.name.clone(),
            })
        })
    }

    fn unpublished(&self) -> impl Iterator<Item = StationCode> {
        Station::all()
            .iter()
            .map(|station| station.code)
            .filter(|code| self.station(*code).is_none())
    }

    fn line_drift(&self, line: Line) -> Option<NetworkDrift> {
        let published: BTreeSet<StationCode> = self
            .routes
            .iter()
            .filter(|route| route.line == line)
            .flat_map(|route| route.stations.iter().copied())
            .collect();
        let compiled: BTreeSet<StationCode> = line
            .stations()
            .iter()
            .copied()
            .filter(|code| self.station(*code).is_some())
            .collect();
        let added: Vec<_> = published.difference(&compiled).copied().collect();
        let removed: Vec<_> = compiled.difference(&published).copied().collect();
        (!added.is_empty() || !removed.is_empty()).then_some(NetworkDrift::LineStations {
            line,
            added,
            removed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    /// The compiled network, published exactly as this service knows it.
    fn compiled() -> PublishedNetwork {
        PublishedNetwork {
            stations: Station::all()
                .iter()
                .map(|station| PublishedStation {
                    code: station.code,
                    name: Localized::new(station.name.en.to_owned(), station.name.tc.to_owned()),
                })
                .collect(),
            routes: Line::with_next_train()
                .map(|line| Route {
                    line,
                    direction: Direction::Up,
                    stations: line.stations().to_vec(),
                })
                .collect(),
        }
    }

    fn without(network: &mut PublishedNetwork, station: &str) {
        network
            .stations
            .retain(|published| published.code != code(station));
        for route in &mut network.routes {
            route.stations.retain(|stop| *stop != code(station));
        }
    }

    #[test]
    fn the_compiled_network_does_not_drift_from_itself() {
        assert_eq!(compiled().drift(), vec![]);
    }

    #[test]
    fn a_station_left_out_everywhere_is_only_unpublished() {
        let mut network = compiled();
        without(&mut network, "RAC");

        assert_eq!(
            network.drift(),
            vec![NetworkDrift::Unpublished(code("RAC"))]
        );
    }

    #[test]
    fn reports_new_and_renamed_stations() {
        let mut network = compiled();
        network.stations.push(PublishedStation {
            code: code("XYZ"),
            name: Localized::new("New Town".to_owned(), "新市鎮".to_owned()),
        });
        if let Some(lai_king) = network.stations.iter_mut().find(|s| s.code == code("LAK")) {
            lai_king.name.tc = "茘景".to_owned();
        }
        network.stations.sort_by_key(|station| station.code);

        assert_eq!(
            network.drift(),
            vec![
                NetworkDrift::Renamed {
                    station: code("LAK"),
                    published: Localized::new("Lai King".to_owned(), "茘景".to_owned()),
                },
                NetworkDrift::UnknownStation(code("XYZ")),
            ]
        );
    }

    #[test]
    fn reports_stations_moved_between_lines() {
        let mut network = compiled();
        for route in &mut network.routes {
            if route.line == Line::KwunTong {
                route.stations.retain(|stop| *stop != code("WHA"));
            }
            if route.line == Line::TsuenWan {
                route.stations.push(code("WHA"));
            }
        }

        assert_eq!(
            network.drift(),
            vec![
                NetworkDrift::LineStations {
                    line: Line::TsuenWan,
                    added: vec![code("WHA")],
                    removed: vec![],
                },
                NetworkDrift::LineStations {
                    line: Line::KwunTong,
                    added: vec![],
                    removed: vec![code("WHA")],
                },
            ]
        );
    }
}
