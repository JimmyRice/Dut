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

    /// The direction trains run on the other track.
    pub const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
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

    /// Where each direction heads, as platform signs name it: up trains on
    /// the Tseung Kwan O Line run towards Po Lam and LOHAS Park.
    ///
    /// Trains that turn back short of these, such as those ending at Tiu
    /// Keng Leng, still report their own destination.
    pub const fn termini(self) -> &'static ByDirection<&'static [StationCode]> {
        &self.profile().termini
    }

    /// The termini a train leaving `station` in `direction` can still reach.
    ///
    /// Empty when the station ends that direction: Po Lam for up trains, but
    /// also LOHAS Park, whose branch never reaches Po Lam.
    ///
    /// # Examples
    ///
    /// The Tseung Kwan O Line forks after Tseung Kwan O station, so up trains
    /// from before the fork reach both termini, and those past it only one.
    ///
    /// ```
    /// use dut_core::domain::network::{Direction, Line, StationCode};
    ///
    /// let tiu_keng_leng: StationCode = "TIK".parse()?;
    /// let hang_hau: StationCode = "HAH".parse()?;
    /// let lohas_park: StationCode = "LHP".parse()?;
    /// let towards = |station, direction| -> Vec<&'static str> {
    ///     Line::TseungKwanO
    ///         .towards(station, direction)
    ///         .map(StationCode::as_str)
    ///         .collect()
    /// };
    ///
    /// assert_eq!(towards(tiu_keng_leng, Direction::Up), ["POA", "LHP"]);
    /// assert_eq!(towards(hang_hau, Direction::Up), ["POA"]);
    /// assert!(towards(lohas_park, Direction::Up).is_empty());
    /// assert_eq!(towards(lohas_park, Direction::Down), ["NOP"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn towards(
        self,
        station: StationCode,
        direction: Direction,
    ) -> impl Iterator<Item = &'static StationCode> {
        self.termini()
            .get(direction)
            .iter()
            .filter(move |terminus| self.leads(station, **terminus, direction))
    }

    /// Whether a train running in `direction` from `from` can go on to `to`:
    /// both lie on one branch, `to` beyond `from`.
    ///
    /// Branches are what make this more than comparing positions in
    /// [`stations`](Self::stations): on the Tseung Kwan O Line, LOHAS Park
    /// is listed between Tseung Kwan O and Hang Hau but lies on its own
    /// branch.
    ///
    /// # Examples
    ///
    /// ```
    /// use dut_core::domain::network::{Direction, Line, StationCode};
    ///
    /// let tseung_kwan_o: StationCode = "TKO".parse()?;
    /// let lohas_park: StationCode = "LHP".parse()?;
    /// let po_lam: StationCode = "POA".parse()?;
    /// let line = Line::TseungKwanO;
    ///
    /// assert!(line.leads(tseung_kwan_o, lohas_park, Direction::Up));
    /// assert!(line.leads(tseung_kwan_o, po_lam, Direction::Up));
    /// assert!(!line.leads(lohas_park, po_lam, Direction::Up));
    /// assert!(line.leads(po_lam, tseung_kwan_o, Direction::Down));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn leads(self, from: StationCode, to: StationCode, direction: Direction) -> bool {
        self.profile().leads(from, to, direction)
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
    layout: Layout,
    termini: ByDirection<&'static [StationCode]>,
}

impl LineProfile {
    /// Whether a train running in `direction` calls at `from` and later
    /// reaches `to`.
    fn leads(&self, from: StationCode, to: StationCode, direction: Direction) -> bool {
        match self.layout {
            Layout::Straight => Branch(self.stations).leads(from, to, direction),
            Layout::Reversed => Branch(self.stations).leads(from, to, direction.opposite()),
            Layout::Forked(branches) => branches
                .iter()
                .any(|branch| branch.leads(from, to, direction)),
        }
    }
}

/// How trains run along a line's `stations`.
///
/// The Next Train API names directions `UP` and `DOWN` without saying which
/// way they point, and the station order alone cannot tell where a fork
/// splits, so each line states it here.
enum Layout {
    /// One track end to end; up trains run towards the last station.
    Straight,
    /// One track end to end; up trains run towards the first station.
    Reversed,
    /// The line splits, so trains follow one of these branches.
    Forked(&'static [Branch]),
}

/// One end-to-end path along a line, listed in the up direction.
struct Branch(&'static [StationCode]);

impl Branch {
    fn leads(&self, from: StationCode, to: StationCode, direction: Direction) -> bool {
        let position = |code| self.0.iter().position(|station| *station == code);
        match (position(from), position(to)) {
            (Some(from), Some(to)) => match direction {
                Direction::Up => from < to,
                Direction::Down => from > to,
            },
            _ => false,
        }
    }
}

// Station order follows the Next Train API data dictionary (v1.7), which also
// defines up and down. Termini follow platform signs. Names and colours follow
// the MTR line status feed.

const AIRPORT_EXPRESS: LineProfile = LineProfile {
    code: "AEL",
    name: Localized::new("Airport Express", "機場快綫"),
    color: "#1C7670",
    stations: codes!["HOK", "KOW", "TSY", "AIR", "AWE"],
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["AIR", "AWE"], codes!["HOK"]),
};

const TUNG_CHUNG: LineProfile = LineProfile {
    code: "TCL",
    name: Localized::new("Tung Chung Line", "東涌綫"),
    color: "#FE7F1D",
    stations: codes!["HOK", "KOW", "OLY", "NAC", "LAK", "TSY", "SUN", "TUC"],
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["TUC"], codes!["HOK"]),
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
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["TUM"], codes!["WKS"]),
};

const TSEUNG_KWAN_O: LineProfile = LineProfile {
    code: "TKL",
    name: Localized::new("Tseung Kwan O Line", "將軍澳綫"),
    color: "#6B208B",
    stations: codes!["NOP", "QUB", "YAT", "TIK", "TKO", "LHP", "HAH", "POA"],
    // LOHAS Park sits on its own branch from Tseung Kwan O.
    layout: Layout::Forked(&[
        Branch(codes!["NOP", "QUB", "YAT", "TIK", "TKO", "HAH", "POA"]),
        Branch(codes!["NOP", "QUB", "YAT", "TIK", "TKO", "LHP"]),
    ]),
    termini: ByDirection::new(codes!["POA", "LHP"], codes!["NOP"]),
};

const EAST_RAIL: LineProfile = LineProfile {
    code: "EAL",
    name: Localized::new("East Rail Line", "東鐵綫"),
    color: "#5EB6E4",
    stations: codes![
        "ADM", "EXC", "HUH", "MKK", "KOT", "TAW", "SHT", "FOT", "RAC", "UNI", "TAP", "TWO", "FAN",
        "SHS", "LOW", "LMC",
    ],
    // The line splits after Sheung Shui for Lo Wu and Lok Ma Chau. Racecourse
    // trains bypass Fo Tan, but both lie on the shared trunk.
    layout: Layout::Forked(&[
        Branch(codes![
            "ADM", "EXC", "HUH", "MKK", "KOT", "TAW", "SHT", "FOT", "RAC", "UNI", "TAP", "TWO",
            "FAN", "SHS", "LOW",
        ]),
        Branch(codes![
            "ADM", "EXC", "HUH", "MKK", "KOT", "TAW", "SHT", "FOT", "RAC", "UNI", "TAP", "TWO",
            "FAN", "SHS", "LMC",
        ]),
    ]),
    termini: ByDirection::new(codes!["LOW", "LMC"], codes!["ADM"]),
};

const SOUTH_ISLAND: LineProfile = LineProfile {
    code: "SIL",
    name: Localized::new("South Island Line", "南港島綫"),
    color: "#99CF16",
    stations: codes!["ADM", "OCP", "WCH", "LET", "SOH"],
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["SOH"], codes!["ADM"]),
};

const TSUEN_WAN: LineProfile = LineProfile {
    code: "TWL",
    name: Localized::new("Tsuen Wan Line", "荃灣綫"),
    color: "#FF0000",
    stations: codes![
        "CEN", "ADM", "TST", "JOR", "YMT", "MOK", "PRE", "SSP", "CSW", "LCK", "MEF", "LAK", "KWF",
        "KWH", "TWH", "TSW",
    ],
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["TSW"], codes!["CEN"]),
};

const ISLAND: LineProfile = LineProfile {
    code: "ISL",
    name: Localized::new("Island Line", "港島綫"),
    color: "#0860A8",
    stations: codes![
        "KET", "HKU", "SYP", "SHW", "CEN", "ADM", "WAC", "CAB", "TIH", "FOH", "NOP", "QUB", "TAK",
        "SWH", "SKW", "HFC", "CHW",
    ],
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["CHW"], codes!["KET"]),
};

const KWUN_TONG: LineProfile = LineProfile {
    code: "KTL",
    name: Localized::new("Kwun Tong Line", "觀塘綫"),
    color: "#1A9431",
    stations: codes![
        "WHA", "HOM", "YMT", "MOK", "PRE", "SKM", "KOT", "LOF", "WTS", "DIH", "CHH", "KOB", "NTK",
        "KWT", "LAT", "YAT", "TIK",
    ],
    layout: Layout::Straight,
    termini: ByDirection::new(codes!["TIK"], codes!["WHA"]),
};

const DISNEYLAND_RESORT: LineProfile = LineProfile {
    code: "DRL",
    name: Localized::new("Disneyland Resort Line", "迪士尼綫"),
    color: "#F550A6",
    stations: codes!["SUN", "DIS"],
    // Unlike every other line, up trains run towards the first station.
    layout: Layout::Reversed,
    termini: ByDirection::new(codes!["SUN"], codes!["DIS"]),
};

const LIGHT_RAIL: LineProfile = LineProfile {
    code: "LR",
    name: Localized::new("Light Rail", "輕鐵"),
    color: "#9F7A00",
    stations: codes![],
    layout: Layout::Straight,
    termini: ByDirection::new(codes![], codes![]),
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

    fn codes(codes: &[&str]) -> Vec<StationCode> {
        codes.iter().map(|station| code(station)).collect()
    }

    fn towards(line: Line, station: &str, direction: Direction) -> Vec<StationCode> {
        line.towards(code(station), direction).copied().collect()
    }

    fn termini(line: Line) -> impl Iterator<Item = &'static StationCode> {
        let termini = line.termini();
        termini.up.iter().chain(termini.down.iter())
    }

    #[test]
    fn every_listed_station_is_known() {
        for line in Line::ALL {
            for station in line.stations().iter().chain(termini(line)) {
                assert!(
                    Station::find(*station).is_some(),
                    "{line} lists unknown station {station}"
                );
            }
        }
    }

    #[test]
    fn termini_are_stations_on_the_line() {
        for line in Line::with_next_train() {
            for station in termini(line) {
                assert!(line.serves(*station), "{line} does not serve {station}");
            }
        }
    }

    #[test]
    fn branches_cover_exactly_the_stations_of_their_line() {
        for line in Line::ALL {
            if let Layout::Forked(branches) = line.profile().layout {
                let on_branches: HashSet<StationCode> = branches
                    .iter()
                    .flat_map(|branch| branch.0.iter().copied())
                    .collect();
                let on_line: HashSet<StationCode> = line.stations().iter().copied().collect();
                assert_eq!(on_branches, on_line, "{line}");
            }
        }
    }

    /// A direction may only run out at one of its own termini. This catches a
    /// layout that points the wrong way or a fork that is missing a branch.
    #[test]
    fn a_direction_only_ends_at_its_own_termini() {
        for line in Line::with_next_train() {
            for &station in line.stations() {
                for direction in Direction::ALL {
                    if line.towards(station, direction).next().is_none() {
                        assert!(
                            line.termini().get(direction).contains(&station),
                            "{line} {direction:?} runs out at {station}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_fork_lists_only_the_termini_its_branch_reaches() {
        let line = Line::TseungKwanO;

        assert_eq!(towards(line, "TKO", Direction::Up), codes(&["POA", "LHP"]));
        assert_eq!(towards(line, "HAH", Direction::Up), codes(&["POA"]));
        assert_eq!(towards(line, "LHP", Direction::Up), codes(&[]));
        assert_eq!(towards(line, "LHP", Direction::Down), codes(&["NOP"]));
    }

    #[test]
    fn a_terminus_has_nowhere_further_to_go() {
        assert_eq!(towards(Line::TseungKwanO, "POA", Direction::Up), codes(&[]));
        assert_eq!(towards(Line::EastRail, "LOW", Direction::Up), codes(&[]));
        assert_eq!(
            towards(Line::EastRail, "SHS", Direction::Up),
            codes(&["LOW", "LMC"])
        );
        assert_eq!(towards(Line::EastRail, "ADM", Direction::Down), codes(&[]));
    }

    #[test]
    fn a_short_working_terminus_is_not_a_platform_sign() {
        assert_eq!(
            towards(Line::TseungKwanO, "YAT", Direction::Down),
            codes(&["NOP"])
        );
        assert_eq!(
            towards(Line::EastRail, "TAW", Direction::Down),
            codes(&["ADM"])
        );
    }

    #[test]
    fn disneyland_resort_up_trains_run_towards_the_first_station() {
        let line = Line::DisneylandResort;

        assert_eq!(towards(line, "DIS", Direction::Up), codes(&["SUN"]));
        assert_eq!(towards(line, "SUN", Direction::Up), codes(&[]));
        assert_eq!(towards(line, "SUN", Direction::Down), codes(&["DIS"]));
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
