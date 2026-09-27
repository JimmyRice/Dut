use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::domain::{localized::Localized, network::StationCode};

/// Expands to a static slice of station codes, validated at compile time.
macro_rules! codes {
    ($($code:literal),* $(,)?) => {
        &[$(StationCode::from_static($code)),*]
    };
}

/// The MTR's two running directions, as used by the Next Train API.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    pub const ALL: [Self; 2] = [Self::Up, Self::Down];
}

/// One value per running direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ByDirection<T> {
    pub up: T,
    pub down: T,
}

impl<T> ByDirection<T> {
    pub const fn new(up: T, down: T) -> Self {
        Self { up, down }
    }

    pub const fn get(&self, direction: Direction) -> &T {
        match direction {
            Direction::Up => &self.up,
            Direction::Down => &self.down,
        }
    }
}

/// A railway line that appears in MTR's real-time feeds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Line {
    AirportExpress,
    TungChung,
    TuenMa,
    TseungKwanO,
    EastRail,
    SouthIsland,
    TsuenWan,
    Island,
    KwunTong,
    DisneylandResort,
    /// Light Rail appears in the line status feed but has no Next Train data.
    LightRail,
}

impl Line {
    pub const ALL: [Self; 11] = [
        Self::AirportExpress,
        Self::TungChung,
        Self::TuenMa,
        Self::TseungKwanO,
        Self::EastRail,
        Self::SouthIsland,
        Self::TsuenWan,
        Self::Island,
        Self::KwunTong,
        Self::DisneylandResort,
        Self::LightRail,
    ];

    /// The MTR line code, such as `TKL`.
    pub const fn code(self) -> &'static str {
        self.profile().code
    }

    pub const fn name(self) -> Localized<&'static str> {
        self.profile().name
    }

    /// The MTR brand colour as a `#RRGGBB` hex string.
    pub const fn color(self) -> &'static str {
        self.profile().color
    }

    /// Stations in the order the data dictionary lists them, which follows
    /// the line end to end. Empty for lines without Next Train data.
    pub const fn stations(self) -> &'static [StationCode] {
        self.profile().stations
    }

    /// Destinations the Next Train API reports for each direction.
    pub const fn destinations(self) -> &'static ByDirection<&'static [StationCode]> {
        &self.profile().destinations
    }

    /// Whether the Next Train API publishes arrivals for this line.
    pub const fn has_next_train(self) -> bool {
        !self.stations().is_empty()
    }

    pub fn serves(self, station: StationCode) -> bool {
        self.stations().contains(&station)
    }

    /// Lines with Next Train data that stop at `station`.
    pub fn serving(station: StationCode) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |line| line.serves(station))
    }

    /// Lines with Next Train data, in the data dictionary's order.
    pub fn with_next_train() -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(|line| line.has_next_train())
    }

    const fn profile(self) -> &'static LineProfile {
        match self {
            Self::AirportExpress => &AIRPORT_EXPRESS,
            Self::TungChung => &TUNG_CHUNG,
            Self::TuenMa => &TUEN_MA,
            Self::TseungKwanO => &TSEUNG_KWAN_O,
            Self::EastRail => &EAST_RAIL,
            Self::SouthIsland => &SOUTH_ISLAND,
            Self::TsuenWan => &TSUEN_WAN,
            Self::Island => &ISLAND,
            Self::KwunTong => &KWUN_TONG,
            Self::DisneylandResort => &DISNEYLAND_RESORT,
            Self::LightRail => &LIGHT_RAIL,
        }
    }
}

/// Parses a line code case-insensitively.
impl FromStr for Line {
    type Err = UnknownLineCode;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|line| line.code().eq_ignore_ascii_case(input))
            .ok_or(UnknownLineCode)
    }
}

impl fmt::Display for Line {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("unknown MTR line code")]
pub struct UnknownLineCode;

/// Static reference data for one line.
struct LineProfile {
    code: &'static str,
    name: Localized<&'static str>,
    color: &'static str,
    stations: &'static [StationCode],
    destinations: ByDirection<&'static [StationCode]>,
}

// Station order and destinations follow the Next Train API data dictionary
// (v1.7). Names and colours follow the MTR line status feed.

const AIRPORT_EXPRESS: LineProfile = LineProfile {
    code: "AEL",
    name: Localized::new("Airport Express", "機場快綫"),
    color: "#1C7670",
    stations: codes!["HOK", "KOW", "TSY", "AIR", "AWE"],
    destinations: ByDirection::new(codes!["AIR", "AWE"], codes!["HOK"]),
};

const TUNG_CHUNG: LineProfile = LineProfile {
    code: "TCL",
    name: Localized::new("Tung Chung Line", "東涌綫"),
    color: "#FE7F1D",
    stations: codes!["HOK", "KOW", "OLY", "NAC", "LAK", "TSY", "SUN", "TUC"],
    destinations: ByDirection::new(codes!["TUC"], codes!["HOK"]),
};

const TUEN_MA: LineProfile = LineProfile {
    code: "TML",
    name: Localized::new("Tuen Ma Line", "屯馬綫"),
    color: "#9A3B26",
    stations: codes![
        "WKS", "MOS", "HEO", "TSH", "SHM", "CIO", "STW", "CKT", "TAW", "HIK", "DIH", "KAT", "SUW",
        "TKW", "HOM", "HUH", "ETS", "AUS", "NAC", "MEF", "TWW", "KSR", "YUL", "LOP", "TIS", "SIH",
        "TUM",
    ],
    destinations: ByDirection::new(codes!["TUM"], codes!["WKS"]),
};

const TSEUNG_KWAN_O: LineProfile = LineProfile {
    code: "TKL",
    name: Localized::new("Tseung Kwan O Line", "將軍澳綫"),
    color: "#6B208B",
    stations: codes!["NOP", "QUB", "YAT", "TIK", "TKO", "LHP", "HAH", "POA"],
    destinations: ByDirection::new(codes!["POA", "LHP"], codes!["TIK", "NOP"]),
};

const EAST_RAIL: LineProfile = LineProfile {
    code: "EAL",
    name: Localized::new("East Rail Line", "東鐵綫"),
    color: "#5EB6E4",
    stations: codes![
        "ADM", "EXC", "HUH", "MKK", "KOT", "TAW", "SHT", "FOT", "RAC", "UNI", "TAP", "TWO", "FAN",
        "SHS", "LOW", "LMC",
    ],
    destinations: ByDirection::new(
        codes!["LMC", "LOW", "SHS", "TAP", "RAC", "FOT", "SHT"],
        codes!["ADM", "HUH", "MKK"],
    ),
};

const SOUTH_ISLAND: LineProfile = LineProfile {
    code: "SIL",
    name: Localized::new("South Island Line", "南港島綫"),
    color: "#99CF16",
    stations: codes!["ADM", "OCP", "WCH", "LET", "SOH"],
    destinations: ByDirection::new(codes!["SOH"], codes!["ADM"]),
};

const TSUEN_WAN: LineProfile = LineProfile {
    code: "TWL",
    name: Localized::new("Tsuen Wan Line", "荃灣綫"),
    color: "#FF0000",
    stations: codes![
        "CEN", "ADM", "TST", "JOR", "YMT", "MOK", "PRE", "SSP", "CSW", "LCK", "MEF", "LAK", "KWF",
        "KWH", "TWH", "TSW",
    ],
    destinations: ByDirection::new(codes!["TSW"], codes!["CEN"]),
};

const ISLAND: LineProfile = LineProfile {
    code: "ISL",
    name: Localized::new("Island Line", "港島綫"),
    color: "#0860A8",
    stations: codes![
        "KET", "HKU", "SYP", "SHW", "CEN", "ADM", "WAC", "CAB", "TIH", "FOH", "NOP", "QUB", "TAK",
        "SWH", "SKW", "HFC", "CHW",
    ],
    destinations: ByDirection::new(codes!["CHW"], codes!["KET"]),
};

const KWUN_TONG: LineProfile = LineProfile {
    code: "KTL",
    name: Localized::new("Kwun Tong Line", "觀塘綫"),
    color: "#1A9431",
    stations: codes![
        "WHA", "HOM", "YMT", "MOK", "PRE", "SKM", "KOT", "LOF", "WTS", "DIH", "CHH", "KOB", "NTK",
        "KWT", "LAT", "YAT", "TIK",
    ],
    destinations: ByDirection::new(codes!["TIK"], codes!["WHA"]),
};

const DISNEYLAND_RESORT: LineProfile = LineProfile {
    code: "DRL",
    name: Localized::new("Disneyland Resort Line", "迪士尼綫"),
    color: "#F550A6",
    stations: codes!["SUN", "DIS"],
    destinations: ByDirection::new(codes!["SUN"], codes!["DIS"]),
};

const LIGHT_RAIL: LineProfile = LineProfile {
    code: "LR",
    name: Localized::new("Light Rail", "輕鐵"),
    color: "#9F7A00",
    stations: codes![],
    destinations: ByDirection::new(codes![], codes![]),
};

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::domain::network::Station;

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    #[test]
    fn parses_line_codes_case_insensitively() {
        assert_eq!("tkl".parse(), Ok(Line::TseungKwanO));
        assert_eq!("LR".parse(), Ok(Line::LightRail));
        assert_eq!("XYZ".parse::<Line>(), Err(UnknownLineCode));
    }

    #[test]
    fn line_codes_round_trip() {
        for line in Line::ALL {
            assert_eq!(line.code().parse(), Ok(line));
        }
    }

    #[test]
    fn every_listed_station_is_known() {
        for line in Line::ALL {
            let destinations = line.destinations();
            let listed = line
                .stations()
                .iter()
                .chain(destinations.up.iter())
                .chain(destinations.down.iter());
            for station in listed {
                assert!(
                    Station::find(*station).is_some(),
                    "{line} lists unknown station {station}"
                );
            }
        }
    }

    #[test]
    fn destinations_are_stations_on_the_line() {
        for line in Line::with_next_train() {
            let destinations = line.destinations();
            for station in destinations.up.iter().chain(destinations.down.iter()) {
                assert!(line.serves(*station), "{line} does not serve {station}");
            }
        }
    }

    #[test]
    fn next_train_covers_every_known_station_exactly() {
        let served: HashSet<StationCode> = Line::with_next_train()
            .flat_map(|line| line.stations().iter().copied())
            .collect();
        let known: HashSet<StationCode> =
            Station::all().iter().map(|station| station.code).collect();

        assert_eq!(served, known);
    }

    #[test]
    fn finds_every_line_at_an_interchange() {
        let lines: Vec<Line> = Line::serving(code("ADM")).collect();

        assert_eq!(
            lines,
            [
                Line::EastRail,
                Line::SouthIsland,
                Line::TsuenWan,
                Line::Island
            ]
        );
    }

    #[test]
    fn light_rail_has_no_next_train_data() {
        assert!(!Line::LightRail.has_next_train());
        assert!(!Line::with_next_train().any(|line| line == Line::LightRail));
    }
}
