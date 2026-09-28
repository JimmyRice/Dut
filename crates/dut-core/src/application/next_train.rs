//! Use cases for upcoming train arrivals.

use std::{fmt, future::Future, sync::Arc};

use futures::future::join_all;
use jiff::Timestamp;
use thiserror::Error;

use crate::{
    application::source::{Freshness, Snapshot, SourceUnavailable},
    domain::{
        network::{Direction, Line, Station, StationCode},
        next_train::{NextTrainBoard, TrainArrival},
    },
};

/// Port for reading Next Train boards.
///
/// Implementations decide how data is fetched and cached; callers only see
/// snapshots with their freshness.
pub trait NextTrainSource: Send + Sync + 'static {
    fn board(
        &self,
        line: Line,
        station: StationCode,
    ) -> impl Future<Output = Result<Snapshot<NextTrainBoard>, SourceUnavailable>> + Send;
}

#[derive(Debug, Error)]
pub enum NextTrainError {
    #[error("station {0} is not in the network")]
    UnknownStation(StationCode),

    #[error("{line} does not serve {station}")]
    StationNotOnLine { line: Line, station: StationCode },

    #[error("next train data is unavailable")]
    Unavailable(#[from] SourceUnavailable),
}

/// A board as it should be presented at a specific instant.
///
/// Trains that have departed by `as_of` are hidden, so a cached board keeps
/// showing only trains that can still be caught.
#[derive(Clone, Debug)]
pub struct BoardView {
    snapshot: Snapshot<NextTrainBoard>,
    as_of: Timestamp,
}

impl BoardView {
    pub const fn new(snapshot: Snapshot<NextTrainBoard>, as_of: Timestamp) -> Self {
        Self { snapshot, as_of }
    }

    pub fn board(&self) -> &NextTrainBoard {
        self.snapshot.value()
    }

    pub const fn snapshot(&self) -> &Snapshot<NextTrainBoard> {
        &self.snapshot
    }

    pub fn upcoming(&self, direction: Direction) -> impl Iterator<Item = &TrainArrival> {
        self.board().upcoming(direction, self.as_of)
    }

    /// Directions a rider can board in at this instant; see
    /// [`NextTrainBoard::directions`].
    pub fn directions(&self) -> impl Iterator<Item = Direction> + '_ {
        self.board().directions(self.as_of)
    }
}

/// The boards of every line at one station.
#[derive(Debug)]
pub struct StationBoards {
    station: StationCode,
    lines: Vec<LineBoard>,
}

/// One line's board at a station, or why it could not be loaded.
#[derive(Debug)]
pub struct LineBoard {
    pub line: Line,
    pub board: Result<BoardView, SourceUnavailable>,
}

impl StationBoards {
    pub const fn station(&self) -> &StationCode {
        &self.station
    }

    pub fn lines(&self) -> &[LineBoard] {
        &self.lines
    }

    /// The combined freshness of the boards that loaded.
    pub fn freshness(&self) -> Freshness {
        self.lines
            .iter()
            .filter_map(|entry| entry.board.as_ref().ok())
            .map(|view| view.snapshot().freshness())
            .reduce(Freshness::combine)
            .unwrap_or(Freshness::Stale)
    }
}

pub struct NextTrainService<S> {
    source: Arc<S>,
}

impl<S: NextTrainSource> NextTrainService<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    /// The board for `station` on `line`.
    ///
    /// The line and station are validated against the static network before
    /// any upstream call, which keeps the set of cache keys bounded.
    pub async fn board(
        &self,
        line: Line,
        station: StationCode,
    ) -> Result<BoardView, NextTrainError> {
        ensure_known(station)?;
        if !line.serves(station) {
            return Err(NextTrainError::StationNotOnLine { line, station });
        }

        let snapshot = self.source.board(line, station).await?;
        Ok(BoardView::new(snapshot, Timestamp::now()))
    }

    /// The boards of every line serving `station`, fetched concurrently.
    ///
    /// A line that fails does not hide the others; the call only fails when
    /// no line could be loaded.
    pub async fn station_boards(
        &self,
        station: StationCode,
    ) -> Result<StationBoards, NextTrainError> {
        ensure_known(station)?;

        let fetches = Line::serving(station)
            .map(|line| async move { (line, self.source.board(line, station).await) });
        let results = join_all(fetches).await;

        let as_of = Timestamp::now();
        let lines: Vec<LineBoard> = results
            .into_iter()
            .map(|(line, result)| LineBoard {
                line,
                board: result.map(|snapshot| BoardView::new(snapshot, as_of)),
            })
            .collect();

        if lines.iter().all(|entry| entry.board.is_err()) {
            let cause = lines.into_iter().find_map(|entry| entry.board.err());
            return Err(cause.map_or(
                NextTrainError::UnknownStation(station),
                NextTrainError::Unavailable,
            ));
        }

        Ok(StationBoards { station, lines })
    }
}

fn ensure_known(station: StationCode) -> Result<(), NextTrainError> {
    match Station::find(station) {
        Some(_) => Ok(()),
        None => Err(NextTrainError::UnknownStation(station)),
    }
}

// Implemented by hand so that neither requires the same of `S`.
impl<S> Clone for NextTrainService<S> {
    fn clone(&self) -> Self {
        Self {
            source: Arc::clone(&self.source),
        }
    }
}

impl<S> fmt::Debug for NextTrainService<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NextTrainService")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use super::*;
    use crate::domain::network::ByDirection;

    /// Serves an empty board for every request except the listed failures,
    /// and records which boards were requested.
    #[derive(Default)]
    struct FakeSource {
        failing: HashSet<Line>,
        requests: Mutex<Vec<(Line, StationCode)>>,
    }

    impl FakeSource {
        fn failing(lines: impl IntoIterator<Item = Line>) -> Self {
            Self {
                failing: lines.into_iter().collect(),
                ..Self::default()
            }
        }

        fn requests(&self) -> Vec<(Line, StationCode)> {
            self.requests
                .lock()
                .expect("request log should not be poisoned")
                .clone()
        }
    }

    impl NextTrainSource for Arc<FakeSource> {
        async fn board(
            &self,
            line: Line,
            station: StationCode,
        ) -> Result<Snapshot<NextTrainBoard>, SourceUnavailable> {
            self.requests
                .lock()
                .expect("request log should not be poisoned")
                .push((line, station));

            if self.failing.contains(&line) {
                return Err(SourceUnavailable::new("upstream failed"));
            }

            let board = NextTrainBoard {
                line,
                station,
                generated_at: Timestamp::UNIX_EPOCH,
                delayed: false,
                alert: None,
                trains: ByDirection::default(),
            };
            let freshness = Freshness::Fresh {
                expires_in: Duration::from_secs(5),
            };
            Ok(Snapshot::new(
                Arc::new(board),
                Timestamp::UNIX_EPOCH,
                freshness,
            ))
        }
    }

    fn service(source: &Arc<FakeSource>) -> NextTrainService<Arc<FakeSource>> {
        NextTrainService::new(Arc::clone(source))
    }

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    #[tokio::test]
    async fn board_reads_from_the_source() {
        let source = Arc::new(FakeSource::default());

        let view = service(&source)
            .board(Line::TseungKwanO, code("TKO"))
            .await
            .expect("board should load");

        assert_eq!(view.board().station, code("TKO"));
        assert_eq!(source.requests(), [(Line::TseungKwanO, code("TKO"))]);
    }

    #[tokio::test]
    async fn board_rejects_a_station_off_the_line_without_calling_upstream() {
        let source = Arc::new(FakeSource::default());

        let error = service(&source)
            .board(Line::TseungKwanO, code("ADM"))
            .await
            .expect_err("ADM is not on TKL");

        assert!(matches!(
            error,
            NextTrainError::StationNotOnLine {
                line: Line::TseungKwanO,
                ..
            }
        ));
        assert!(source.requests().is_empty());
    }

    #[tokio::test]
    async fn board_rejects_an_unknown_station() {
        let source = Arc::new(FakeSource::default());

        let error = service(&source)
            .board(Line::TseungKwanO, code("XYZ"))
            .await
            .expect_err("XYZ is not a station");

        assert!(matches!(error, NextTrainError::UnknownStation(_)));
        assert!(source.requests().is_empty());
    }

    #[tokio::test]
    async fn station_boards_cover_every_line_at_an_interchange() {
        let source = Arc::new(FakeSource::default());

        let boards = service(&source)
            .station_boards(code("ADM"))
            .await
            .expect("boards should load");

        let lines: Vec<Line> = boards.lines().iter().map(|entry| entry.line).collect();
        assert_eq!(
            lines,
            [
                Line::EastRail,
                Line::SouthIsland,
                Line::TsuenWan,
                Line::Island
            ]
        );
        assert_eq!(source.requests().len(), 4);
    }

    #[tokio::test]
    async fn station_boards_keep_working_lines_when_one_fails() {
        let source = Arc::new(FakeSource::failing([Line::SouthIsland]));

        let boards = service(&source)
            .station_boards(code("ADM"))
            .await
            .expect("other lines should still load");

        let failed: Vec<Line> = boards
            .lines()
            .iter()
            .filter(|entry| entry.board.is_err())
            .map(|entry| entry.line)
            .collect();
        assert_eq!(failed, [Line::SouthIsland]);
        assert!(!boards.freshness().is_stale());
    }

    #[tokio::test]
    async fn station_boards_fail_when_every_line_fails() {
        let source = Arc::new(FakeSource::failing([Line::TseungKwanO]));

        let error = service(&source)
            .station_boards(code("TKO"))
            .await
            .expect_err("the only line failed");

        assert!(matches!(error, NextTrainError::Unavailable(_)));
    }
}
