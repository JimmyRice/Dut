//! Upcoming train arrivals for one station on one line.

use std::{fmt, str::FromStr};

use jiff::{SignedDuration, Timestamp};
use thiserror::Error;

use crate::domain::{
    localized::Localized,
    network::{ByDirection, Direction, Line, StationCode},
};

/// How long after its estimated time a train is still shown.
///
/// Trains dwell at the platform for a short while, so a train whose estimate
/// has just passed is usually still boarding.
pub const DEPARTED_GRACE: SignedDuration = SignedDuration::from_secs(30);

/// Whether an East Rail Line estimate refers to arrival or departure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeType {
    Arrival,
    Departure,
}

/// One upcoming train, identified by its absolute estimated time.
///
/// Countdown values are deliberately absent: they go stale as soon as the
/// data is cached, whereas the absolute time stays correct until the MTR
/// revises its estimate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrainArrival {
    /// Position among the upcoming trains, starting at 1.
    pub sequence: u8,
    pub destination: StationCode,
    pub platforms: Platforms,
    /// Estimated arrival time, or departure time when `time_type` says so.
    pub arrival_at: Timestamp,
    /// Only published for the East Rail Line.
    pub time_type: Option<TimeType>,
    /// East Rail Line trains that run via Racecourse instead of Fo Tan.
    pub via_racecourse: bool,
}

impl TrainArrival {
    /// Whether the train has left the platform as of `as_of`.
    pub fn has_departed(&self, as_of: Timestamp) -> bool {
        as_of.duration_since(self.arrival_at) > DEPARTED_GRACE
    }
}

/// The platforms a train stands at, in the order the MTR lists them.
///
/// A train almost always stands at one platform. At Airport, each Airport
/// Express track has a platform on either side and trains open their doors on
/// both, so the MTR lists two, such as `1/3`. A train whose platform the MTR
/// published illegibly has none, so that its time can still be shown.
///
/// # Examples
///
/// ```
/// use dut_core::domain::next_train::Platforms;
///
/// let airport: Platforms = "1/3".parse()?;
/// assert_eq!(airport.as_slice(), [1, 3]);
/// assert_eq!("2".parse::<Platforms>()?, Platforms::one(2));
///
/// // No train stands at more than two platforms.
/// assert!("1/2/3".parse::<Platforms>().is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy)]
pub struct Platforms {
    numbers: [u8; 2],
    len: u8,
}

impl Platforms {
    /// No known platform.
    pub const NONE: Self = Self {
        numbers: [0; 2],
        len: 0,
    };

    /// A train at one platform, as nearly everywhere.
    pub const fn one(number: u8) -> Self {
        Self {
            numbers: [number, 0],
            len: 1,
        }
    }

    /// A train between two platforms with its doors open on both sides, as
    /// at Airport.
    pub const fn pair(first: u8, second: u8) -> Self {
        Self {
            numbers: [first, second],
            len: 2,
        }
    }

    /// The platform numbers, empty when none is known.
    pub fn as_slice(&self) -> &[u8] {
        // `len` never exceeds the array, so this never falls back.
        self.numbers
            .get(..usize::from(self.len))
            .unwrap_or_default()
    }
}

/// Parses platforms as the MTR publishes them: one number, or two joined by
/// a slash.
impl FromStr for Platforms {
    type Err = InvalidPlatforms;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let number = |text: &str| text.trim().parse().map_err(|_| InvalidPlatforms);
        match input.split_once('/') {
            None => Ok(Self::one(number(input)?)),
            Some((first, second)) => Ok(Self::pair(number(first)?, number(second)?)),
        }
    }
}

impl PartialEq for Platforms {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Platforms {}

impl fmt::Debug for Platforms {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Platforms({:?})", self.as_slice())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("platforms must be one number, or two joined by a slash")]
pub struct InvalidPlatforms;

/// A special arrangement notice published instead of, or alongside, the
/// regular schedule.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AlertNotice {
    pub message: String,
    pub url: Option<String>,
}

/// The Next Train board for one station on one line, as published by the MTR
/// at `generated_at`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NextTrainBoard {
    pub line: Line,
    pub station: StationCode,
    /// When the MTR generated this data, which can be older than when it was
    /// fetched.
    pub generated_at: Timestamp,
    pub delayed: bool,
    pub alert: Option<Localized<AlertNotice>>,
    /// Trains per direction, ordered by `sequence`.
    pub trains: ByDirection<Vec<TrainArrival>>,
}

impl NextTrainBoard {
    /// Trains in `direction` that have not yet departed as of `as_of`.
    pub fn upcoming(
        &self,
        direction: Direction,
        as_of: Timestamp,
    ) -> impl Iterator<Item = &TrainArrival> {
        self.trains
            .get(direction)
            .iter()
            .filter(move |train| !train.has_departed(as_of))
    }

    /// Directions a rider can board in at this station as of `as_of`.
    ///
    /// A direction is shown while it leads to a terminus beyond the station,
    /// so Po Lam has no board towards Po Lam. It is also shown whenever the
    /// MTR reports trains in it, so no train is hidden by the static network.
    pub fn directions(&self, as_of: Timestamp) -> impl Iterator<Item = Direction> + '_ {
        Direction::ALL.into_iter().filter(move |&direction| {
            self.line.towards(self.station, direction).next().is_some()
                || self.upcoming(direction, as_of).next().is_some()
        })
    }

    /// What this board says about service beyond its train times.
    pub fn signal(&self) -> NextTrainSignal {
        NextTrainSignal {
            delayed: self.delayed,
            notice: self.alert.clone(),
        }
    }
}

/// What a Next Train board says about service beyond its train times.
///
/// The Next Train API flags delays and publishes special arrangement notices
/// per station, independently of the line status feed, so a sampled board
/// may show trouble before or after the line status does.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NextTrainSignal {
    pub delayed: bool,
    pub notice: Option<Localized<AlertNotice>>,
}

/// The signal of one sampled board.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardSignal {
    pub line: Line,
    pub station: StationCode,
    pub signal: NextTrainSignal,
}

/// The signals of the boards sampled in one round.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NextTrainSignals {
    pub boards: Vec<BoardSignal>,
}

/// A sampled board whose signal differs between two rounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalChange {
    pub line: Line,
    pub station: StationCode,
    pub previous: NextTrainSignal,
    pub current: NextTrainSignal,
}

impl NextTrainSignals {
    /// The boards whose signal changed since `previous`. A board missing from
    /// either round, such as one that failed to load, is left out.
    pub fn changes_since(&self, previous: &Self) -> Vec<SignalChange> {
        self.boards
            .iter()
            .filter_map(|current| {
                let before = previous
                    .boards
                    .iter()
                    .find(|board| board.line == current.line && board.station == current.station)?;
                (before.signal != current.signal).then(|| SignalChange {
                    line: current.line,
                    station: current.station,
                    previous: before.signal.clone(),
                    current: current.signal.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::station;

    fn timestamp(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).expect("test timestamp should be in range")
    }

    fn train(sequence: u8, arrival_at: Timestamp) -> TrainArrival {
        TrainArrival {
            sequence,
            destination: station!("POA"),
            platforms: Platforms::one(1),
            arrival_at,
            time_type: None,
            via_racecourse: false,
        }
    }

    #[test]
    fn a_train_is_kept_during_the_departure_grace_period() {
        let arrival_at = timestamp(1_000);
        let train = train(1, arrival_at);

        assert!(!train.has_departed(timestamp(900)));
        assert!(!train.has_departed(arrival_at));
        assert!(!train.has_departed(arrival_at + DEPARTED_GRACE));
        assert!(train.has_departed(arrival_at + DEPARTED_GRACE + SignedDuration::from_secs(1)));
    }

    #[test]
    fn platforms_parse_one_number_or_two_joined_by_a_slash() {
        let parse = |input: &str| input.parse::<Platforms>();

        assert_eq!(parse("1"), Ok(Platforms::one(1)));
        assert_eq!(parse("1/3"), Ok(Platforms::pair(1, 3)));
        assert_eq!(parse(" 2 / 4 "), Ok(Platforms::pair(2, 4)));
    }

    #[test]
    fn platforms_reject_anything_else() {
        for input in ["", "A", "1/", "/3", "1/2/3", "1,3", "-1", "256"] {
            assert_eq!(input.parse::<Platforms>(), Err(InvalidPlatforms), "{input}");
        }
    }

    #[test]
    fn platforms_compare_by_their_numbers() {
        assert_eq!(Platforms::NONE.as_slice(), [0; 0]);
        assert_ne!(Platforms::one(1), Platforms::pair(1, 3));
        assert_ne!(Platforms::NONE, Platforms::one(0));
        assert_eq!(format!("{:?}", Platforms::pair(1, 3)), "Platforms([1, 3])");
    }

    fn board(station: StationCode, trains: ByDirection<Vec<TrainArrival>>) -> NextTrainBoard {
        NextTrainBoard {
            line: Line::TseungKwanO,
            station,
            generated_at: timestamp(0),
            delayed: false,
            alert: None,
            trains,
        }
    }

    #[test]
    fn upcoming_skips_departed_trains_in_the_requested_direction() {
        let board = board(
            station!("TKO"),
            ByDirection::new(
                vec![train(1, timestamp(100)), train(2, timestamp(400))],
                vec![train(1, timestamp(500))],
            ),
        );

        let upcoming: Vec<u8> = board
            .upcoming(Direction::Up, timestamp(300))
            .map(|train| train.sequence)
            .collect();

        assert_eq!(upcoming, [2]);
    }

    #[test]
    fn a_mid_line_station_shows_both_directions_even_without_trains() {
        let board = board(station!("TKO"), ByDirection::default());

        let directions: Vec<Direction> = board.directions(timestamp(0)).collect();

        assert_eq!(directions, [Direction::Up, Direction::Down]);
    }

    #[test]
    fn a_terminus_leaves_out_the_direction_that_ends_there() {
        let board = board(station!("POA"), ByDirection::default());

        let directions: Vec<Direction> = board.directions(timestamp(0)).collect();

        assert_eq!(directions, [Direction::Down]);
    }

    #[test]
    fn a_direction_with_upcoming_trains_is_never_left_out() {
        let arriving = train(1, timestamp(100));
        let board = board(
            station!("POA"),
            ByDirection::new(vec![arriving], Vec::new()),
        );

        let before: Vec<Direction> = board.directions(timestamp(0)).collect();
        let after: Vec<Direction> = board.directions(timestamp(200)).collect();

        assert_eq!(before, [Direction::Up, Direction::Down]);
        assert_eq!(after, [Direction::Down]);
    }

    fn notice(message: &str) -> Localized<AlertNotice> {
        let notice = AlertNotice {
            message: message.to_owned(),
            url: None,
        };
        Localized::new(notice.clone(), notice)
    }

    #[test]
    fn a_board_signals_its_delay_flag_and_notice() {
        let board = NextTrainBoard {
            delayed: true,
            alert: Some(notice("Special train service arrangement")),
            ..board(station!("TKO"), ByDirection::default())
        };

        assert_eq!(
            board.signal(),
            NextTrainSignal {
                delayed: true,
                notice: Some(notice("Special train service arrangement")),
            }
        );
    }

    fn sampled(station: StationCode, signal: NextTrainSignal) -> BoardSignal {
        BoardSignal {
            line: Line::TseungKwanO,
            station,
            signal,
        }
    }

    fn delayed() -> NextTrainSignal {
        NextTrainSignal {
            delayed: true,
            notice: None,
        }
    }

    #[test]
    fn signal_changes_cover_only_boards_whose_signal_changed() {
        let before = NextTrainSignals {
            boards: vec![
                sampled(station!("TKO"), NextTrainSignal::default()),
                sampled(station!("POA"), NextTrainSignal::default()),
            ],
        };
        let after = NextTrainSignals {
            boards: vec![
                sampled(station!("TKO"), delayed()),
                sampled(station!("POA"), NextTrainSignal::default()),
            ],
        };

        let changes = after.changes_since(&before);

        assert_eq!(
            changes,
            [SignalChange {
                line: Line::TseungKwanO,
                station: station!("TKO"),
                previous: NextTrainSignal::default(),
                current: delayed(),
            }]
        );
    }

    #[test]
    fn a_board_that_failed_to_load_is_not_a_signal_change() {
        let before = NextTrainSignals {
            boards: vec![sampled(station!("TKO"), delayed())],
        };
        let after = NextTrainSignals::default();

        assert!(after.changes_since(&before).is_empty());
    }
}
