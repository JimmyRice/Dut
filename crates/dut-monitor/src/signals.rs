//! Samples Next Train boards for delay flags and special arrangement notices.

use std::{error::Error, fmt};

use futures::future::join_all;
use tracing::debug;

use dut_core::{
    application::{
        feed::Feed,
        next_train::{NextTrainService, NextTrainSource},
        source::SourceUnavailable,
    },
    domain::{
        network::{Line, StationCode},
        next_train::{BoardSignal, NextTrainSignals},
        source_health::SourceId,
    },
};

/// The station sampled on each line with Next Train data.
///
/// Each is a mid-line station served in both directions, so trouble in
/// either direction shows on its board. The Disneyland Resort Line has only
/// its two termini, so Sunny Bay stands in.
const SENTINELS: [(Line, StationCode); 10] = [
    (Line::AirportExpress, StationCode::from_static("TSY")),
    (Line::TungChung, StationCode::from_static("LAK")),
    (Line::TuenMa, StationCode::from_static("NAC")),
    (Line::TseungKwanO, StationCode::from_static("TKO")),
    (Line::EastRail, StationCode::from_static("KOT")),
    (Line::SouthIsland, StationCode::from_static("WCH")),
    (Line::TsuenWan, StationCode::from_static("MOK")),
    (Line::Island, StationCode::from_static("ADM")),
    (Line::KwunTong, StationCode::from_static("KOT")),
    (Line::DisneylandResort, StationCode::from_static("SUN")),
];

/// The Next Train signal at one station per line.
///
/// Boards are read through the same cached source that serves riders, so
/// sampling a board a rider has just asked for costs no upstream request.
/// A round succeeds when any board loads; boards that fail are left out of
/// it, so they show no change.
pub struct NextTrainSignalFeed<S> {
    next_trains: NextTrainService<S>,
}

impl<S> NextTrainSignalFeed<S> {
    pub const fn new(next_trains: NextTrainService<S>) -> Self {
        Self { next_trains }
    }
}

// Implemented by hand so that it does not require the same of `S`.
impl<S> fmt::Debug for NextTrainSignalFeed<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NextTrainSignalFeed")
            .field("next_trains", &self.next_trains)
            .finish()
    }
}

impl<S: NextTrainSource> Feed for NextTrainSignalFeed<S> {
    type Item = NextTrainSignals;

    const SOURCE: SourceId = SourceId::MtrNextTrain;

    async fn fetch(&self) -> Result<NextTrainSignals, SourceUnavailable> {
        let reads = SENTINELS.iter().map(|&(line, station)| async move {
            (line, station, self.next_trains.board(line, station).await)
        });

        let mut boards = Vec::with_capacity(SENTINELS.len());
        let mut failure = None;
        for (line, station, result) in join_all(reads).await {
            match result {
                Ok(view) => boards.push(BoardSignal {
                    line,
                    station,
                    signal: view.board().signal(),
                }),
                Err(board_error) => {
                    debug!(
                        %line,
                        %station,
                        error = &board_error as &dyn Error,
                        "sampled board is unavailable"
                    );
                    failure.get_or_insert(board_error);
                }
            }
        }

        match failure {
            Some(board_error) if boards.is_empty() => Err(SourceUnavailable::new(board_error)),
            _ => Ok(NextTrainSignals { boards }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc, time::Duration};

    use jiff::Timestamp;

    use dut_core::{
        application::source::{Freshness, Snapshot},
        domain::{
            network::{ByDirection, Direction},
            next_train::NextTrainBoard,
        },
    };

    use super::*;

    #[test]
    fn samples_one_station_on_every_line_with_next_train_data() {
        let sampled: HashSet<Line> = SENTINELS.iter().map(|&(line, _)| line).collect();
        let expected: HashSet<Line> = Line::with_next_train().collect();

        assert_eq!(sampled, expected);
        assert_eq!(sampled.len(), SENTINELS.len());
    }

    #[test]
    fn sampled_stations_are_served_in_both_directions_where_possible() {
        for (line, station) in SENTINELS {
            assert!(line.serves(station), "{line} does not serve {station}");
            let directions = Direction::ALL
                .into_iter()
                .filter(|&direction| line.towards(station, direction).next().is_some())
                .count();
            if line != Line::DisneylandResort {
                assert_eq!(directions, 2, "{line} at {station} is a terminus");
            }
        }
    }

    /// Serves a board flagged as delayed on the given lines and fails on
    /// the others listed as failing.
    struct FakeSource {
        delayed: HashSet<Line>,
        failing: HashSet<Line>,
    }

    impl NextTrainSource for FakeSource {
        async fn board(
            &self,
            line: Line,
            station: StationCode,
        ) -> Result<Snapshot<NextTrainBoard>, SourceUnavailable> {
            if self.failing.contains(&line) {
                return Err(SourceUnavailable::new("upstream failed"));
            }
            let board = NextTrainBoard {
                line,
                station,
                generated_at: Timestamp::UNIX_EPOCH,
                delayed: self.delayed.contains(&line),
                alert: None,
                trains: ByDirection::default(),
            };
            let freshness = Freshness::Fresh {
                expires_in: Duration::from_secs(5),
            };
            Ok(Snapshot::new(Arc::new(board), Timestamp::now(), freshness))
        }
    }

    fn feed(
        delayed: impl IntoIterator<Item = Line>,
        failing: impl IntoIterator<Item = Line>,
    ) -> NextTrainSignalFeed<FakeSource> {
        NextTrainSignalFeed::new(NextTrainService::new(FakeSource {
            delayed: delayed.into_iter().collect(),
            failing: failing.into_iter().collect(),
        }))
    }

    #[tokio::test]
    async fn reads_the_signal_of_every_sampled_board() {
        let signals = feed([Line::KwunTong], [])
            .fetch()
            .await
            .expect("every board loads");

        assert_eq!(signals.boards.len(), SENTINELS.len());
        let delayed: Vec<Line> = signals
            .boards
            .iter()
            .filter(|board| board.signal.delayed)
            .map(|board| board.line)
            .collect();
        assert_eq!(delayed, [Line::KwunTong]);
    }

    #[tokio::test]
    async fn leaves_out_boards_that_fail() {
        let signals = feed([], [Line::Island])
            .fetch()
            .await
            .expect("other boards load");

        assert_eq!(signals.boards.len(), SENTINELS.len() - 1);
        assert!(
            !signals
                .boards
                .iter()
                .any(|board| board.line == Line::Island)
        );
    }

    #[tokio::test]
    async fn fails_when_every_board_fails() {
        let result = feed([], Line::with_next_train()).fetch().await;

        assert!(result.is_err());
    }
}
