//! How each line is run: the workings its trains follow in turn, how often
//! they come in each part of the day, and how long they take between
//! stations.
//!
//! Workings and their mix follow the live Next Train API: every board on the
//! morning of 2026-10-02, at 07:05 and in the peak at 07:45, and boards from
//! the evening of 2026-09-27. Headways follow those captures, rounded.

use dut_core::domain::network::{ByDirection, Direction, Line, StationCode};

/// The part of the day a timetable is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Period {
    Peak,
    OffPeak,
    LateNight,
}

/// Where one train starts and where it ends. A working that ends short of
/// the line's terminus, such as a Kwun Tong Line train to Ho Man Tin, is a
/// short working.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Working {
    pub(crate) from: StationCode,
    pub(crate) to: StationCode,
}

const FO_TAN: StationCode = StationCode::from_static("FOT");
const RACECOURSE: StationCode = StationCode::from_static("RAC");

impl Working {
    /// Whether a train on this working stops at `station`. East Rail Line
    /// trains call at Fo Tan, or at Racecourse on race days, never both.
    pub(crate) fn calls_at(
        self,
        line: Line,
        station: StationCode,
        direction: Direction,
        via_racecourse: bool,
    ) -> bool {
        let skipped = if via_racecourse { FO_TAN } else { RACECOURSE };
        let on_route = station == self.from
            || station == self.to
            || (line.leads(self.from, station, direction)
                && line.leads(station, self.to, direction));
        on_route && station != skipped
    }
}

/// One value for each part of the day.
#[derive(Debug)]
struct ByPeriod<T> {
    peak: T,
    off_peak: T,
    late_night: T,
}

impl<T> ByPeriod<T> {
    const fn get(&self, period: Period) -> &T {
        match period {
            Period::Peak => &self.peak,
            Period::OffPeak => &self.off_peak,
            Period::LateNight => &self.late_night,
        }
    }
}

/// The workings trains in each direction follow in turn.
type Workings = ByDirection<&'static [Working]>;

/// How one line is run.
#[derive(Debug)]
pub(crate) struct Timetable {
    /// Seconds between trains through the busiest section.
    headways: ByPeriod<i64>,
    /// Seconds from one station to the next, on average.
    hop: i64,
    /// A station on every branch. Trains are timed by when they pass it, so
    /// trains on different workings keep their spacing where they share
    /// track.
    reference: StationCode,
    workings: ByPeriod<Workings>,
}

impl Timetable {
    /// The timetable of a line with Next Train data.
    pub(crate) const fn of(line: Line) -> Option<&'static Self> {
        match line {
            Line::AirportExpress => Some(&AIRPORT_EXPRESS),
            Line::TungChung => Some(&TUNG_CHUNG),
            Line::TuenMa => Some(&TUEN_MA),
            Line::TseungKwanO => Some(&TSEUNG_KWAN_O),
            Line::EastRail => Some(&EAST_RAIL),
            Line::SouthIsland => Some(&SOUTH_ISLAND),
            Line::TsuenWan => Some(&TSUEN_WAN),
            Line::Island => Some(&ISLAND),
            Line::KwunTong => Some(&KWUN_TONG),
            Line::DisneylandResort => Some(&DISNEYLAND_RESORT),
            Line::LightRail => None,
        }
    }

    /// Seconds between trains in `period`.
    pub(crate) const fn headway(&self, period: Period) -> i64 {
        *self.headways.get(period)
    }

    /// The workings trains in `direction` follow in turn during `period`.
    pub(crate) const fn workings(
        &self,
        period: Period,
        direction: Direction,
    ) -> &'static [Working] {
        self.workings.get(period).get(direction)
    }

    /// Seconds from when a train in `direction` passes the reference station
    /// to when it reaches `station`, negative for stations before it.
    /// `None` for a station off the line.
    pub(crate) fn offset(
        &self,
        line: Line,
        station: StationCode,
        direction: Direction,
    ) -> Option<i64> {
        let hops = if station == self.reference {
            Some(0)
        } else if line.leads(self.reference, station, direction) {
            hops(line, self.reference, station, direction)
        } else {
            hops(line, station, self.reference, direction).map(|hops| -hops)
        };
        hops.map(|hops| hops * self.hop)
    }
}

/// How many stations a train in `direction` reaches from `from` up to and
/// including `to`, or `None` if `to` does not lie beyond `from`.
///
/// Fo Tan and Racecourse lie side by side, and a train calls at one of them,
/// so they count once.
pub(crate) fn hops(
    line: Line,
    from: StationCode,
    to: StationCode,
    direction: Direction,
) -> Option<i64> {
    if from == to {
        return Some(0);
    }
    if !line.leads(from, to, direction) {
        return None;
    }
    let path: Vec<StationCode> = line
        .stations()
        .iter()
        .copied()
        .filter(|&station| {
            line.leads(from, station, direction)
                && (station == to || line.leads(station, to, direction))
        })
        .collect();
    let side_by_side = path.contains(&FO_TAN) && path.contains(&RACECOURSE);
    Some(path.len() as i64 - i64::from(side_by_side))
}

/// A working, checked at compile time.
const fn working(from: &str, to: &str) -> Working {
    Working {
        from: StationCode::from_static(from),
        to: StationCode::from_static(to),
    }
}

/// Workings that do not change with the time of day.
const fn all_day(workings: Workings) -> ByPeriod<Workings> {
    ByPeriod {
        peak: workings,
        off_peak: workings,
        late_night: workings,
    }
}

const fn headways(peak: i64, off_peak: i64, late_night: i64) -> ByPeriod<i64> {
    ByPeriod {
        peak,
        off_peak,
        late_night,
    }
}

const AIRPORT_EXPRESS: Timetable = Timetable {
    headways: headways(600, 600, 720),
    hop: 420,
    reference: StationCode::from_static("TSY"),
    // Every train of both captures ran on to AsiaWorld-Expo.
    workings: all_day(ByDirection::new(
        &[working("HOK", "AWE")],
        &[working("AWE", "HOK")],
    )),
};

const TUNG_CHUNG_THROUGHOUT: Workings =
    ByDirection::new(&[working("HOK", "TUC")], &[working("TUC", "HOK")]);

const TUNG_CHUNG: Timetable = Timetable {
    headways: headways(300, 450, 600),
    hop: 180,
    reference: StationCode::from_static("TSY"),
    workings: ByPeriod {
        // One train in three turns back at Tsing Yi.
        peak: ByDirection::new(
            &[
                working("HOK", "TUC"),
                working("HOK", "TSY"),
                working("HOK", "TUC"),
            ],
            &[
                working("TUC", "HOK"),
                working("TSY", "HOK"),
                working("TUC", "HOK"),
            ],
        ),
        off_peak: TUNG_CHUNG_THROUGHOUT,
        late_night: TUNG_CHUNG_THROUGHOUT,
    },
};

const TUEN_MA: Timetable = Timetable {
    headways: headways(165, 270, 450),
    hop: 135,
    reference: StationCode::from_static("HUH"),
    workings: all_day(ByDirection::new(
        &[working("WKS", "TUM")],
        &[working("TUM", "WKS")],
    )),
};

/// One train in three runs to LOHAS Park.
const TSEUNG_KWAN_O_BY_DAY: Workings = ByDirection::new(
    &[
        working("NOP", "POA"),
        working("NOP", "LHP"),
        working("NOP", "POA"),
    ],
    &[
        working("POA", "NOP"),
        working("LHP", "NOP"),
        working("POA", "NOP"),
    ],
);

const TSEUNG_KWAN_O: Timetable = Timetable {
    headways: headways(140, 240, 360),
    hop: 120,
    reference: StationCode::from_static("TKO"),
    workings: ByPeriod {
        peak: TSEUNG_KWAN_O_BY_DAY,
        off_peak: TSEUNG_KWAN_O_BY_DAY,
        // LOHAS Park trains shuttle to and from Tiu Keng Leng, where riders
        // change to the Kwun Tong Line.
        late_night: ByDirection::new(
            &[
                working("NOP", "POA"),
                working("NOP", "POA"),
                working("TIK", "LHP"),
            ],
            &[
                working("POA", "NOP"),
                working("POA", "NOP"),
                working("LHP", "TIK"),
            ],
        ),
    },
};

const EAST_RAIL: Timetable = Timetable {
    headways: headways(170, 300, 450),
    hop: 170,
    reference: StationCode::from_static("SHS"),
    workings: ByPeriod {
        // One train in four turns back at Tai Po Market, and one of the
        // others runs to Lok Ma Chau.
        peak: ByDirection::new(
            &[
                working("ADM", "LMC"),
                working("ADM", "LOW"),
                working("ADM", "LOW"),
                working("ADM", "TAP"),
            ],
            &[
                working("LMC", "ADM"),
                working("LOW", "ADM"),
                working("LOW", "ADM"),
                working("TAP", "ADM"),
            ],
        ),
        // One train in three runs to Lok Ma Chau.
        off_peak: ByDirection::new(
            &[
                working("ADM", "LMC"),
                working("ADM", "LOW"),
                working("ADM", "LOW"),
            ],
            &[
                working("LMC", "ADM"),
                working("LOW", "ADM"),
                working("LOW", "ADM"),
            ],
        ),
        // One train in three turns back at Sheung Shui.
        late_night: ByDirection::new(
            &[
                working("ADM", "LOW"),
                working("ADM", "SHS"),
                working("ADM", "LMC"),
            ],
            &[
                working("LOW", "ADM"),
                working("SHS", "ADM"),
                working("LMC", "ADM"),
            ],
        ),
    },
};

const SOUTH_ISLAND: Timetable = Timetable {
    headways: headways(200, 270, 420),
    hop: 120,
    reference: StationCode::from_static("WCH"),
    workings: all_day(ByDirection::new(
        &[working("ADM", "SOH")],
        &[working("SOH", "ADM")],
    )),
};

const TSUEN_WAN: Timetable = Timetable {
    headways: headways(125, 210, 360),
    hop: 110,
    reference: StationCode::from_static("MOK"),
    workings: all_day(ByDirection::new(
        &[working("CEN", "TSW")],
        &[working("TSW", "CEN")],
    )),
};

const ISLAND: Timetable = Timetable {
    headways: headways(150, 210, 360),
    hop: 100,
    reference: StationCode::from_static("ADM"),
    workings: all_day(ByDirection::new(
        &[working("KET", "CHW")],
        &[working("CHW", "KET")],
    )),
};

/// Every other train starts or ends at Ho Man Tin instead of Whampoa.
const KWUN_TONG_BY_DAY: Workings = ByDirection::new(
    &[working("WHA", "TIK"), working("HOM", "TIK")],
    &[working("TIK", "WHA"), working("TIK", "HOM")],
);

const KWUN_TONG: Timetable = Timetable {
    headways: headways(125, 210, 360),
    hop: 105,
    reference: StationCode::from_static("KOT"),
    workings: ByPeriod {
        peak: KWUN_TONG_BY_DAY,
        off_peak: KWUN_TONG_BY_DAY,
        late_night: ByDirection::new(&[working("WHA", "TIK")], &[working("TIK", "WHA")]),
    },
};

const DISNEYLAND_RESORT: Timetable = Timetable {
    headways: headways(360, 480, 600),
    hop: 270,
    reference: StationCode::from_static("SUN"),
    workings: all_day(ByDirection::new(
        &[working("DIS", "SUN")],
        &[working("SUN", "DIS")],
    )),
};

#[cfg(test)]
mod tests {
    use super::*;

    const PERIODS: [Period; 3] = [Period::Peak, Period::OffPeak, Period::LateNight];

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    fn timetables() -> impl Iterator<Item = (Line, &'static Timetable)> {
        Line::with_next_train().filter_map(|line| Timetable::of(line).map(|table| (line, table)))
    }

    #[test]
    fn every_line_with_next_train_data_has_a_timetable() {
        assert_eq!(timetables().count(), Line::with_next_train().count());
        assert!(Timetable::of(Line::LightRail).is_none());
    }

    #[test]
    fn workings_run_along_their_line_in_their_direction() {
        for (line, timetable) in timetables() {
            for period in PERIODS {
                for direction in Direction::ALL {
                    let workings = timetable.workings(period, direction);
                    assert!(!workings.is_empty(), "{line} {direction:?}");
                    for working in workings {
                        assert!(
                            line.leads(working.from, working.to, direction),
                            "{line} {direction:?} {working:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn headways_lengthen_as_the_day_goes_on() {
        for (line, timetable) in timetables() {
            let [peak, off_peak, late_night] = PERIODS.map(|period| timetable.headway(period));
            assert!(
                0 < peak && peak <= off_peak && off_peak <= late_night,
                "{line}"
            );
        }
    }

    /// Every station must be timed from the reference, so it has to lie on
    /// one branch with it.
    #[test]
    fn every_station_is_timed_from_the_reference() {
        for (line, timetable) in timetables() {
            for &station in line.stations() {
                for direction in Direction::ALL {
                    assert!(
                        timetable.offset(line, station, direction).is_some(),
                        "{line} {direction:?} {station}"
                    );
                }
            }
        }
    }

    /// Except at Racecourse, which opens on race days only.
    #[test]
    fn some_train_serves_every_station_in_every_direction_it_leads() {
        for (line, timetable) in timetables() {
            for period in PERIODS {
                for &station in line.stations() {
                    for direction in Direction::ALL {
                        if station == RACECOURSE
                            || line.towards(station, direction).next().is_none()
                        {
                            continue;
                        }
                        let served = timetable.workings(period, direction).iter().any(|working| {
                            working.to != station
                                && working.calls_at(line, station, direction, false)
                        });
                        assert!(served, "{line} {period:?} {direction:?} {station}");
                    }
                }
            }
        }
    }

    #[test]
    fn stations_further_along_are_reached_later() {
        let timetable = &TSEUNG_KWAN_O;
        let offset = |station, direction| {
            timetable
                .offset(Line::TseungKwanO, code(station), direction)
                .expect("station should be timed")
        };

        assert_eq!(offset("TKO", Direction::Up), 0);
        assert_eq!(offset("NOP", Direction::Up), -4 * 120);
        assert_eq!(offset("LHP", Direction::Up), 120);
        assert_eq!(offset("POA", Direction::Up), 2 * 120);
        assert_eq!(offset("LHP", Direction::Down), -120);
        assert_eq!(offset("NOP", Direction::Down), 4 * 120);
    }

    #[test]
    fn fo_tan_and_racecourse_count_as_one_stop() {
        let line = Line::EastRail;

        assert_eq!(hops(line, code("SHT"), code("UNI"), Direction::Up), Some(2));
        assert_eq!(hops(line, code("SHT"), code("FOT"), Direction::Up), Some(1));
        assert_eq!(hops(line, code("UNI"), code("SHT"), Direction::Up), None);
    }

    #[test]
    fn racecourse_trains_skip_fo_tan() {
        let working = working("ADM", "LOW");
        let calls = |station, via_racecourse| {
            working.calls_at(Line::EastRail, code(station), Direction::Up, via_racecourse)
        };

        assert!(calls("FOT", false) && !calls("RAC", false));
        assert!(calls("RAC", true) && !calls("FOT", true));
        assert!(calls("SHT", true) && calls("SHT", false));
        assert!(!calls("LMC", false));
    }

    #[test]
    fn a_working_on_one_branch_never_calls_at_the_other() {
        let lohas_park = working("NOP", "LHP");
        let calls =
            |station| lohas_park.calls_at(Line::TseungKwanO, code(station), Direction::Up, false);

        assert!(calls("NOP") && calls("TKO") && calls("LHP"));
        assert!(!calls("HAH") && !calls("POA"));
    }
}
