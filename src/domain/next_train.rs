//! Upcoming train arrivals for one station on one line.

use jiff::{SignedDuration, Timestamp};

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
    pub platform: u8,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).expect("test timestamp should be in range")
    }

    fn train(sequence: u8, arrival_at: Timestamp) -> TrainArrival {
        TrainArrival {
            sequence,
            destination: "POA".parse().expect("test station code should be valid"),
            platform: 1,
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

    fn board(station: &str, trains: ByDirection<Vec<TrainArrival>>) -> NextTrainBoard {
        NextTrainBoard {
            line: Line::TseungKwanO,
            station: station.parse().expect("test station code should be valid"),
            generated_at: timestamp(0),
            delayed: false,
            alert: None,
            trains,
        }
    }

    #[test]
    fn upcoming_skips_departed_trains_in_the_requested_direction() {
        let board = board(
            "TKO",
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
        let board = board("TKO", ByDirection::default());

        let directions: Vec<Direction> = board.directions(timestamp(0)).collect();

        assert_eq!(directions, [Direction::Up, Direction::Down]);
    }

    #[test]
    fn a_terminus_leaves_out_the_direction_that_ends_there() {
        let board = board("POA", ByDirection::default());

        let directions: Vec<Direction> = board.directions(timestamp(0)).collect();

        assert_eq!(directions, [Direction::Down]);
    }

    #[test]
    fn a_direction_with_upcoming_trains_is_never_left_out() {
        let arriving = train(1, timestamp(100));
        let board = board("POA", ByDirection::new(vec![arriving], Vec::new()));

        let before: Vec<Direction> = board.directions(timestamp(0)).collect();
        let after: Vec<Direction> = board.directions(timestamp(200)).collect();

        assert_eq!(before, [Direction::Up, Direction::Down]);
        assert_eq!(after, [Direction::Down]);
    }
}
