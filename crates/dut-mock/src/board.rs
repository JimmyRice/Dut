//! Simulated Next Train boards.
//!
//! Each line runs its timetable in both directions: train `n` passes the
//! line's reference station `n` headways after the Unix epoch, plus a phase
//! and a deviation drawn from the seed. A board lists the next trains that
//! call at its station, so a board read later, or at the next station,
//! shows the same trains further along.

use jiff::Timestamp;

use dut_core::domain::{
    network::{ByDirection, Direction, Line, StationCode},
    next_train::{NextTrainBoard, Platforms, TimeType, TrainArrival},
};

use crate::{
    notices, platforms,
    scenario::BoardScenario,
    seed::{Purpose, Seed, text_key},
    timetable::{Period, Timetable, Working},
};

/// How many trains a direction lists, as on the Next Train API.
const LISTED: usize = 4;

/// How long a train stays listed after its time, while it stands at the
/// platform. The Next Train API lists it with no minutes to go.
const AT_PLATFORM: i64 = 20;

/// How many trains from the first candidate on are considered: enough to
/// find four that call at a station served by one train in three, with
/// some cancelled.
const CONSIDERED: i64 = 64;

/// The last trains of the night run again in every window of this length,
/// so polling shows them leave one by one without the board then staying
/// empty for the rest of the day.
const LAST_TRAIN_WINDOW: i64 = 20 * 60;

/// What one board is simulated under.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Conditions {
    service: Service,
    /// The MTR flags the line as delayed, and its trains run late and
    /// unevenly, some of them cancelled.
    delayed: bool,
    /// A special arrangement notice accompanies the board.
    notice: bool,
    /// Some East Rail Line trains run via Racecourse.
    race_day: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Service {
    Running(Period),
    /// The last trains of the night are running.
    Ending(Period),
    Ended,
}

impl Service {
    const fn period(self) -> Option<Period> {
        match self {
            Self::Running(period) | Self::Ending(period) => Some(period),
            Self::Ended => None,
        }
    }
}

impl Conditions {
    /// The conditions of a board in `scenario`. An incident, such as a delay
    /// or a notice, applies only to the `affected` line.
    pub(crate) const fn of(scenario: BoardScenario, affected: bool) -> Self {
        let service = match scenario {
            BoardScenario::Peak | BoardScenario::Delayed => Service::Running(Period::Peak),
            BoardScenario::LateNight => Service::Running(Period::LateNight),
            BoardScenario::LastTrain => Service::Ending(Period::LateNight),
            BoardScenario::NonServiceHours => Service::Ended,
            BoardScenario::OffPeak
            | BoardScenario::SpecialArrangement
            | BoardScenario::RaceDay
            | BoardScenario::Stale
            | BoardScenario::PartialOutage
            | BoardScenario::UpstreamUnavailable => Service::Running(Period::OffPeak),
        };
        Self {
            service,
            delayed: affected && matches!(scenario, BoardScenario::Delayed),
            notice: affected && matches!(scenario, BoardScenario::SpecialArrangement),
            race_day: matches!(scenario, BoardScenario::RaceDay),
        }
    }
}

/// The board for `station` on `line` as the MTR would generate it at
/// `generated_at`.
pub(crate) fn simulate(
    line: Line,
    station: StationCode,
    conditions: Conditions,
    seed: Seed,
    generated_at: Timestamp,
) -> NextTrainBoard {
    let trains = |direction| {
        Timetable::of(line).map_or_else(Vec::new, |timetable| {
            Departures {
                line,
                station,
                direction,
                timetable,
                conditions,
                seed,
            }
            .listed(generated_at.as_second())
        })
    };

    NextTrainBoard {
        line,
        station,
        generated_at,
        delayed: conditions.delayed,
        alert: conditions.notice.then(notices::special_arrangement),
        trains: ByDirection::new(trains(Direction::Up), trains(Direction::Down)),
    }
}

/// The trains leaving one station in one direction.
struct Departures {
    line: Line,
    station: StationCode,
    direction: Direction,
    timetable: &'static Timetable,
    conditions: Conditions,
    seed: Seed,
}

/// One train of a line's timetable.
struct Train {
    /// When it passes the reference station, in Unix seconds.
    passes_reference: i64,
    working: Working,
    cancelled: bool,
    via_racecourse: bool,
}

impl Departures {
    /// The next trains to call at the station, as the Next Train API lists
    /// them at `now`, in Unix seconds.
    fn listed(&self, now: i64) -> Vec<TrainArrival> {
        let Some(period) = self.conditions.service.period() else {
            return Vec::new();
        };
        let Some(offset) = self
            .timetable
            .offset(self.line, self.station, self.direction)
        else {
            return Vec::new();
        };
        let schedule = Schedule::new(self, period);
        let last = self.last_departure(now);

        // Deviations stay within half a headway, so a train numbered before
        // this one cannot still be at the platform.
        let first = (now - AT_PLATFORM - offset - schedule.phase).div_euclid(schedule.headway) - 1;
        let mut listed = Vec::with_capacity(LISTED);
        for number in first..first + CONSIDERED {
            if listed.len() == LISTED {
                break;
            }
            let Some(train) = schedule.train(self, number) else {
                continue;
            };
            if last.is_some_and(|last| train.passes_reference > last) {
                break;
            }
            let due = train.passes_reference + offset;
            if train.cancelled || due < now - AT_PLATFORM || !self.calls(&train) {
                continue;
            }
            listed.extend(self.arrival(&train, number, due, now, listed.len()));
        }
        listed
    }

    /// Whether the train stops here to take on riders: a train that ends
    /// here is not listed.
    fn calls(&self, train: &Train) -> bool {
        train.working.to != self.station
            && train.working.calls_at(
                self.line,
                self.station,
                self.direction,
                train.via_racecourse,
            )
    }

    fn arrival(
        &self,
        train: &Train,
        number: i64,
        due: i64,
        now: i64,
        position: usize,
    ) -> Option<TrainArrival> {
        let turns = platforms::at(self.line, self.station, self.direction);
        let platforms = turns
            .get(number.rem_euclid(turns.len().max(1) as i64) as usize)
            .copied()
            .unwrap_or(Platforms::NONE);
        // The East Rail Line tells arrivals from departures, which are where
        // a train starts.
        let time_type =
            (self.line == Line::EastRail).then_some(if train.working.from == self.station {
                TimeType::Departure
            } else {
                TimeType::Arrival
            });

        Some(TrainArrival {
            sequence: u8::try_from(position + 1).ok()?,
            destination: train.working.to,
            platforms,
            arrival_at: Timestamp::from_second(published(now, due)).ok()?,
            time_type,
            via_racecourse: train.via_racecourse,
        })
    }

    /// When the last train of the night passes the reference station, if
    /// the last trains are running.
    fn last_departure(&self, now: i64) -> Option<i64> {
        let Service::Ending(_) = self.conditions.service else {
            return None;
        };
        let window = now.div_euclid(LAST_TRAIN_WINDOW);
        let mut rng = self.seed.stream(
            Purpose::LastTrain,
            &[self.line_key(), self.direction_key(), window as u64],
        );
        Some(window * LAST_TRAIN_WINDOW + rng.between(2 * 60, LAST_TRAIN_WINDOW - 2 * 60))
    }

    fn line_key(&self) -> u64 {
        text_key(self.line.code())
    }

    const fn direction_key(&self) -> u64 {
        match self.direction {
            Direction::Up => 1,
            Direction::Down => 2,
        }
    }
}

/// When a line's trains in one direction pass its reference station.
struct Schedule {
    workings: &'static [Working],
    headway: i64,
    /// How far from its slot a train may run, either way.
    spread: i64,
    phase: i64,
    /// Which working train 0 follows.
    turn: i64,
}

impl Schedule {
    fn new(departures: &Departures, period: Period) -> Self {
        let conditions = departures.conditions;
        let headway = departures.timetable.headway(period);
        let (headway, spread_percent) = if conditions.delayed {
            (headway * 17 / 10, 40)
        } else {
            (headway, 8)
        };
        let workings = departures.timetable.workings(period, departures.direction);
        let mut start = departures.seed.stream(
            Purpose::Timetable,
            &[departures.line_key(), departures.direction_key()],
        );
        Self {
            workings,
            headway,
            spread: headway * spread_percent / 100,
            phase: start.below(headway as u64) as i64,
            turn: start.below(workings.len() as u64) as i64,
        }
    }

    /// Train `number`. Its draws depend on the line, direction, and number
    /// only, so every station sees the same train.
    fn train(&self, departures: &Departures, number: i64) -> Option<Train> {
        let conditions = departures.conditions;
        let mut rng = departures.seed.stream(
            Purpose::Train,
            &[
                departures.line_key(),
                departures.direction_key(),
                number as u64,
            ],
        );
        let deviation = rng.between(-self.spread, self.spread);
        let cancelled = conditions.delayed && rng.percent(12);
        let via_racecourse =
            conditions.race_day && departures.line == Line::EastRail && rng.percent(35);
        let turn = (number + self.turn).rem_euclid(self.workings.len().max(1) as i64);

        Some(Train {
            passes_reference: number * self.headway + self.phase + deviation,
            working: *self.workings.get(turn as usize)?,
            cancelled,
            via_racecourse,
        })
    }
}

/// The time the Next Train API publishes for a train due at `due`: whole
/// minutes from when it generated the board, and no minutes for a train at
/// the platform.
fn published(generated_at: i64, due: i64) -> i64 {
    let minutes = (due - generated_at + 30).div_euclid(60).max(0);
    generated_at + minutes * 60
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use serde_json::Value;

    use dut_core::station;

    use super::*;

    const SERVICE: [BoardScenario; 3] = [
        BoardScenario::Peak,
        BoardScenario::OffPeak,
        BoardScenario::LateNight,
    ];

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    /// 08:00 HKT on the day of the capture.
    fn morning() -> Timestamp {
        "2026-10-02T00:00:00Z".parse().expect("valid timestamp")
    }

    fn later(seconds: i64) -> Timestamp {
        Timestamp::from_second(morning().as_second() + seconds).expect("valid timestamp")
    }

    fn board(line: Line, station: &str, scenario: BoardScenario, at: Timestamp) -> NextTrainBoard {
        simulate(
            line,
            code(station),
            Conditions::of(scenario, true),
            Seed::DEFAULT,
            at,
        )
    }

    fn destinations(trains: &[TrainArrival]) -> BTreeSet<&str> {
        trains
            .iter()
            .map(|train| train.destination.as_str())
            .collect()
    }

    fn span(trains: &[TrainArrival]) -> i64 {
        match (trains.first(), trains.last()) {
            (Some(first), Some(last)) => last.arrival_at.as_second() - first.arrival_at.as_second(),
            _ => 0,
        }
    }

    #[test]
    fn boards_list_trains_as_the_next_train_api_does() {
        for line in Line::with_next_train() {
            for &station in line.stations() {
                for scenario in SERVICE {
                    for seed in 0..3 {
                        let generated_at = later(seed as i64 * 7_919);
                        let board = simulate(
                            line,
                            station,
                            Conditions::of(scenario, true),
                            Seed::new(seed),
                            generated_at,
                        );
                        for direction in Direction::ALL {
                            check(&board, direction, generated_at);
                        }
                    }
                }
            }
        }
    }

    fn check(board: &NextTrainBoard, direction: Direction, generated_at: Timestamp) {
        let (line, station) = (board.line, board.station);
        let trains = board.trains.get(direction);
        let context = format!("{line} {station} {direction:?}: {trains:?}");
        assert!(trains.len() <= LISTED, "{context}");
        let mut previous = generated_at;
        for (index, train) in trains.iter().enumerate() {
            assert_eq!(usize::from(train.sequence), index + 1, "{context}");
            assert!(train.arrival_at >= previous, "{context}");
            assert_eq!(
                (train.arrival_at.as_second() - generated_at.as_second()) % 60,
                0,
                "{context}"
            );
            assert!(
                line.leads(station, train.destination, direction),
                "{context}"
            );
            assert!(
                platforms::at(line, station, direction).contains(&train.platforms),
                "{context}"
            );
            assert_eq!(
                train.time_type.is_some(),
                line == Line::EastRail,
                "{context}"
            );
            assert!(!train.via_racecourse, "{context}");
            previous = train.arrival_at;
        }
    }

    #[test]
    fn a_running_line_lists_four_trains_away_from_its_termini() {
        let board = board(Line::KwunTong, "KOT", BoardScenario::OffPeak, morning());

        assert_eq!(board.trains.up.len(), 4);
        assert_eq!(board.trains.down.len(), 4);
    }

    #[test]
    fn a_terminus_lists_no_trains_towards_itself() {
        let cases = [
            (Line::TseungKwanO, "POA", Direction::Up),
            (Line::TseungKwanO, "LHP", Direction::Up),
            (Line::EastRail, "LOW", Direction::Up),
            (Line::EastRail, "ADM", Direction::Down),
            (Line::DisneylandResort, "SUN", Direction::Up),
        ];
        for (line, station, direction) in cases {
            let board = board(line, station, BoardScenario::Peak, morning());

            assert!(board.trains.get(direction).is_empty(), "{line} {station}");
            assert!(
                !board.trains.get(direction.opposite()).is_empty(),
                "{line} {station}"
            );
        }
    }

    #[test]
    fn a_branch_station_lists_only_trains_on_its_branch() {
        let up_at_hang_hau = board(Line::TseungKwanO, "HAH", BoardScenario::Peak, morning());
        let down_by_day = board(Line::TseungKwanO, "LHP", BoardScenario::OffPeak, morning());
        let down_at_night = board(
            Line::TseungKwanO,
            "LHP",
            BoardScenario::LateNight,
            morning(),
        );

        assert_eq!(
            destinations(&up_at_hang_hau.trains.up),
            BTreeSet::from(["POA"])
        );
        assert_eq!(
            destinations(&down_by_day.trains.down),
            BTreeSet::from(["NOP"])
        );
        assert_eq!(
            destinations(&down_at_night.trains.down),
            BTreeSet::from(["TIK"])
        );
    }

    /// The trains the live Next Train API listed on the morning of
    /// 2026-10-02, keyed by line, station, and direction.
    fn captured_destinations() -> HashMap<(String, String), BTreeSet<String>> {
        let text = include_str!("../../../tests/fixtures/mtr/next_train_network.json");
        let capture: HashMap<String, Value> =
            serde_json::from_str(text).expect("capture should be valid JSON");
        let mut destinations = HashMap::new();
        for (key, response) in &capture {
            for name in ["UP", "DOWN"] {
                let trains = response["data"][key][name].as_array();
                let seen: BTreeSet<String> = trains
                    .into_iter()
                    .flatten()
                    .filter_map(|train| train["dest"].as_str().map(str::to_owned))
                    .collect();
                if !seen.is_empty() {
                    destinations.insert((key.clone(), name.to_owned()), seen);
                }
            }
        }
        destinations
    }

    #[test]
    fn rush_hour_runs_every_working_the_live_api_showed() {
        let captured = captured_destinations();
        assert_eq!(captured.len(), 218, "the capture should cover every board");
        for line in Line::with_next_train() {
            for &station in line.stations() {
                for (direction, name) in [(Direction::Up, "UP"), (Direction::Down, "DOWN")] {
                    let key = (format!("{line}-{station}"), name.to_owned());
                    let Some(seen) = captured.get(&key) else {
                        continue;
                    };
                    let simulated: BTreeSet<String> = (0..30)
                        .flat_map(|step| {
                            let board = simulate(
                                line,
                                station,
                                Conditions::of(BoardScenario::Peak, true),
                                Seed::DEFAULT,
                                later(step * 120),
                            );
                            board.trains.get(direction).clone()
                        })
                        .map(|train| train.destination.as_str().to_owned())
                        .collect();
                    assert!(
                        simulated.is_superset(seen),
                        "{key:?}: {simulated:?} lacks {seen:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn trains_move_on_with_the_clock() {
        let earlier = board(Line::TsuenWan, "MOK", BoardScenario::OffPeak, morning());
        let two_minutes_on = board(Line::TsuenWan, "MOK", BoardScenario::OffPeak, later(120));

        let horizon = earlier.trains.up.last().map(|train| train.arrival_at);
        for train in &two_minutes_on.trains.up {
            if horizon.is_some_and(|horizon| train.arrival_at > horizon) {
                continue;
            }
            assert!(
                earlier.trains.up.iter().any(|before| {
                    before.destination == train.destination
                        && (before.arrival_at.as_second() - train.arrival_at.as_second()).abs()
                            <= 60
                }),
                "{train:?} was not on the board two minutes earlier: {:?}",
                earlier.trains.up
            );
        }
    }

    #[test]
    fn the_next_station_sees_the_same_train_a_stop_later() {
        let at_kowloon_tong = board(Line::KwunTong, "KOT", BoardScenario::OffPeak, morning());
        let at_lok_fu = board(Line::KwunTong, "LOF", BoardScenario::OffPeak, morning());

        let gaps: Vec<i64> = at_kowloon_tong
            .trains
            .up
            .iter()
            .zip(&at_lok_fu.trains.up)
            .map(|(here, next)| next.arrival_at.as_second() - here.arrival_at.as_second())
            .collect();
        assert!(gaps.iter().all(|gap| (60..=180).contains(gap)), "{gaps:?}");
    }

    #[test]
    fn trains_come_more_often_in_rush_hour_than_late_at_night() {
        let span_at = |scenario| span(&board(Line::Island, "CAB", scenario, morning()).trains.up);

        assert!(span_at(BoardScenario::Peak) < span_at(BoardScenario::OffPeak));
        assert!(span_at(BoardScenario::OffPeak) < span_at(BoardScenario::LateNight));
    }

    #[test]
    fn east_rail_line_trains_depart_where_they_start() {
        let admiralty = board(Line::EastRail, "ADM", BoardScenario::Peak, morning());
        let sha_tin = board(Line::EastRail, "SHT", BoardScenario::Peak, morning());

        assert!(admiralty.trains.up.iter().all(|train| {
            train.time_type == Some(TimeType::Departure) && train.platforms == Platforms::one(7)
        }));
        assert!(
            sha_tin
                .trains
                .up
                .iter()
                .all(|train| train.time_type == Some(TimeType::Arrival))
        );
    }

    #[test]
    fn airport_express_trains_stand_at_two_platforms_at_airport() {
        let airport = board(Line::AirportExpress, "AIR", BoardScenario::Peak, morning());

        assert!(!airport.trains.up.is_empty() && !airport.trains.down.is_empty());
        assert!(
            airport
                .trains
                .up
                .iter()
                .all(|train| train.platforms == Platforms::pair(1, 3))
        );
        assert!(
            airport
                .trains
                .down
                .iter()
                .all(|train| train.platforms == Platforms::pair(2, 4))
        );
    }

    #[test]
    fn racecourse_opens_on_race_days_only() {
        let ordinary = board(Line::EastRail, "RAC", BoardScenario::OffPeak, morning());
        assert!(ordinary.trains.up.is_empty() && ordinary.trains.down.is_empty());

        let race_day = (0..10)
            .map(|step| {
                board(
                    Line::EastRail,
                    "RAC",
                    BoardScenario::RaceDay,
                    later(step * 600),
                )
            })
            .flat_map(|board| board.trains.up)
            .collect::<Vec<_>>();
        assert!(!race_day.is_empty());
        assert!(race_day.iter().all(|train| train.via_racecourse));

        let fo_tan = board(Line::EastRail, "FOT", BoardScenario::RaceDay, morning());
        assert!(!fo_tan.trains.up.is_empty());
        assert!(fo_tan.trains.up.iter().all(|train| !train.via_racecourse));
    }

    #[test]
    fn no_trains_run_outside_service_hours() {
        let board = board(
            Line::TsuenWan,
            "MOK",
            BoardScenario::NonServiceHours,
            morning(),
        );

        assert!(board.trains.up.is_empty() && board.trains.down.is_empty());
        assert_eq!(
            board.directions(morning()).collect::<Vec<_>>(),
            [Direction::Up, Direction::Down]
        );
    }

    /// At reference stations, where the last train is due within its window
    /// wherever the window falls.
    #[test]
    fn once_the_last_train_leaves_none_follows() {
        for (line, station) in [
            (Line::KwunTong, "KOT"),
            (Line::Island, "ADM"),
            (Line::TuenMa, "HUH"),
        ] {
            for direction in Direction::ALL {
                let mut emptied = false;
                for step in 0..LAST_TRAIN_WINDOW / 30 {
                    let board = board(line, station, BoardScenario::LastTrain, later(step * 30));
                    let trains = board.trains.get(direction);
                    assert!(
                        !(emptied && !trains.is_empty()),
                        "{line} {station} {direction:?}"
                    );
                    emptied |= trains.is_empty();
                }
                assert!(emptied, "the last train never left {line} {station}");
            }
        }
    }

    #[test]
    fn the_last_trains_leave_some_directions_empty_and_others_not() {
        let counts: Vec<usize> = Line::with_next_train()
            .flat_map(|line| line.stations().iter().map(move |&station| (line, station)))
            .flat_map(|(line, station)| {
                let board = simulate(
                    line,
                    station,
                    Conditions::of(BoardScenario::LastTrain, true),
                    Seed::DEFAULT,
                    later(600),
                );
                [board.trains.up.len(), board.trains.down.len()]
            })
            .collect();

        assert!(counts.contains(&0));
        assert!(counts.iter().any(|&count| count > 0));
    }

    #[test]
    fn a_delayed_line_is_flagged_and_its_trains_come_less_often() {
        let delayed = board(Line::KwunTong, "KOT", BoardScenario::Delayed, morning());
        let unaffected = simulate(
            Line::KwunTong,
            station!("KOT"),
            Conditions::of(BoardScenario::Delayed, false),
            Seed::DEFAULT,
            morning(),
        );

        assert!(delayed.delayed);
        assert!(!unaffected.delayed);
        let total_span = |scenario| -> i64 {
            (0..20)
                .map(|step| {
                    span(
                        &board(Line::KwunTong, "KOT", scenario, later(step * 300))
                            .trains
                            .up,
                    )
                })
                .sum()
        };
        assert!(total_span(BoardScenario::Delayed) > total_span(BoardScenario::Peak));
    }

    #[test]
    fn only_the_affected_line_carries_the_notice() {
        let affected = board(
            Line::Island,
            "ADM",
            BoardScenario::SpecialArrangement,
            morning(),
        );
        let other = simulate(
            Line::TsuenWan,
            station!("ADM"),
            Conditions::of(BoardScenario::SpecialArrangement, false),
            Seed::DEFAULT,
            morning(),
        );

        assert_eq!(affected.alert, Some(notices::special_arrangement()));
        assert!(!affected.trains.up.is_empty());
        assert_eq!(other.alert, None);
    }

    #[test]
    fn times_are_whole_minutes_from_the_board_time() {
        assert_eq!(published(1_000, 1_000), 1_000);
        assert_eq!(published(1_000, 985), 1_000);
        assert_eq!(published(1_000, 1_029), 1_000);
        assert_eq!(published(1_000, 1_030), 1_060);
        assert_eq!(published(1_000, 1_140), 1_120);
        assert_eq!(published(1_000, 1_150), 1_180);
    }
}
