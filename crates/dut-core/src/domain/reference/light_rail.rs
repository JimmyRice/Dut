use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::domain::{localized::Localized, three_letters::ThreeLetters};

/// A Light Rail stop's number, such as `1` for Tuen Mun Ferry Pier. The
/// MTR's Light Rail real-time API identifies stops by it too.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StopId(u16);

impl StopId {
    pub const fn new(id: u16) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for StopId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A Light Rail stop's three-letter code, such as `TMF`.
///
/// It is a separate type from
/// [`StationCode`](crate::domain::network::StationCode) because the two sets
/// of codes are assigned independently.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StopCode(ThreeLetters);

impl StopCode {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Parses a stop code case-insensitively.
impl FromStr for StopCode {
    type Err = InvalidStopCode;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        ThreeLetters::parse(input.as_bytes())
            .map(Self)
            .ok_or(InvalidStopCode)
    }
}

impl fmt::Debug for StopCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "StopCode({})", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a Light Rail stop code must be three ASCII letters")]
pub struct InvalidStopCode;

/// A Light Rail route as trains and stops show it, such as `505` or `614P`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RouteNumber(Box<str>);

impl RouteNumber {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Accepts one to four ASCII letters or digits, uppercasing letters.
impl FromStr for RouteNumber {
    type Err = InvalidRouteNumber;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let valid = (1..=4).contains(&input.len())
            && input.bytes().all(|byte| byte.is_ascii_alphanumeric());
        valid
            .then(|| Self(input.to_ascii_uppercase().into_boxed_str()))
            .ok_or(InvalidRouteNumber)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a Light Rail route number must be one to four ASCII letters or digits")]
pub struct InvalidRouteNumber;

/// A Light Rail stop, with the name its platforms show.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Stop {
    pub id: StopId,
    pub code: StopCode,
    pub name: Localized<String>,
}

/// A Light Rail route and the stops it calls at in each direction.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LightRailRoute {
    pub number: RouteNumber,
    /// The stops each direction calls at, in order, each list never empty.
    /// Circular routes, such as 705 in Tin Shui Wai, publish their loop as
    /// two halves that meet at the far end.
    pub directions: Vec<Vec<StopId>>,
}

/// The Light Rail network in Tuen Mun, Yuen Long, and Tin Shui Wai.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LightRailNetwork {
    /// Ordered by number.
    pub stops: Vec<Stop>,
    /// In the order the MTR lists them.
    pub routes: Vec<LightRailRoute>,
}

impl LightRailNetwork {
    pub fn stop(&self, id: StopId) -> Option<&Stop> {
        self.stops
            .binary_search_by_key(&id, |stop| stop.id)
            .ok()
            .and_then(|index| self.stops.get(index))
    }

    /// The routes that call at a stop, in the order the MTR lists routes.
    pub fn routes_serving(&self, id: StopId) -> impl Iterator<Item = &RouteNumber> {
        self.routes
            .iter()
            .filter(move |route| route.directions.iter().any(|stops| stops.contains(&id)))
            .map(|route| &route.number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(id: u16, code: &str) -> Stop {
        Stop {
            id: StopId::new(id),
            code: code.parse().expect("test stop code should be valid"),
            name: Localized::default(),
        }
    }

    fn route(number: &str, directions: &[&[u16]]) -> LightRailRoute {
        LightRailRoute {
            number: number.parse().expect("test route number should be valid"),
            directions: directions
                .iter()
                .map(|stops| stops.iter().copied().map(StopId::new).collect())
                .collect(),
        }
    }

    #[test]
    fn parses_route_numbers_with_a_suffix() {
        let number: RouteNumber = "614p".parse().expect("route number should parse");

        assert_eq!(number.as_str(), "614P");
        for input in ["", "61455", "614-P", "六一四"] {
            assert_eq!(
                input.parse::<RouteNumber>(),
                Err(InvalidRouteNumber),
                "{input}"
            );
        }
    }

    #[test]
    fn finds_stops_and_the_routes_serving_them() {
        let network = LightRailNetwork {
            stops: vec![stop(1, "FEP"), stop(10, "MPS"), stop(15, "SAT")],
            routes: vec![
                route("505", &[&[1, 10], &[10, 1]]),
                route("507", &[&[1, 15]]),
            ],
        };

        assert_eq!(
            network.stop(StopId::new(10)).map(|stop| stop.code.as_str()),
            Some("MPS")
        );
        assert!(network.stop(StopId::new(2)).is_none());
        let serving: Vec<_> = network
            .routes_serving(StopId::new(1))
            .map(RouteNumber::as_str)
            .collect();
        assert_eq!(serving, ["505", "507"]);
    }
}
