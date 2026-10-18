//! What each cached response body depends on, so an encoded body is reused
//! exactly while rendering it again would produce the same bytes.

use jiff::Timestamp;

use dut_core::application::{
    next_train::{BoardView, StationBoards},
    source::Snapshot,
};

/// What a body rendered from a snapshot depends on besides the data: the
/// fetch it came from, and whether that fetch has gone stale. Each
/// successful fetch changes `fetched_at`, even when the data stay the same.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FetchVersion {
    fetched_at: Timestamp,
    stale: bool,
}

impl FetchVersion {
    pub(crate) fn of<T>(snapshot: &Snapshot<T>) -> Self {
        Self {
            fetched_at: snapshot.fetched_at(),
            stale: snapshot.freshness().is_stale(),
        }
    }
}

/// What a Next Train board's body depends on: its fetch, and how many of
/// its trains have departed by the time of the request. Departed trains are
/// hidden, so the same snapshot renders differently once a train leaves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BoardVersion {
    fetch: FetchVersion,
    departed: usize,
}

impl BoardVersion {
    pub(crate) fn of(view: &BoardView) -> Self {
        Self {
            fetch: FetchVersion::of(view.snapshot()),
            departed: view.departed(),
        }
    }
}

/// What a station's boards depend on: each line's board, or its absence
/// when that line failed, in the order the station's lines are listed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StationBoardsVersion(Vec<Option<BoardVersion>>);

impl StationBoardsVersion {
    pub(crate) fn of(boards: &StationBoards) -> Self {
        Self(
            boards
                .lines()
                .iter()
                .map(|line| line.board.as_ref().ok().map(BoardVersion::of))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use jiff::SignedDuration;

    use dut_core::{
        application::source::Freshness,
        domain::{
            network::{ByDirection, Line},
            next_train::{DEPARTED_GRACE, NextTrainBoard, Platforms, TrainArrival},
        },
    };

    use super::*;

    fn at(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).expect("valid timestamp")
    }

    fn fresh_for(seconds: u64) -> Freshness {
        Freshness::Fresh {
            expires_in: Duration::from_secs(seconds),
        }
    }

    fn version(fetched_at: Timestamp, freshness: Freshness) -> FetchVersion {
        FetchVersion::of(&Snapshot::new(Arc::new(()), fetched_at, freshness))
    }

    #[test]
    fn a_fetched_body_changes_with_each_fetch_and_when_it_goes_stale() {
        assert_eq!(version(at(0), fresh_for(60)), version(at(0), fresh_for(1)));
        assert_ne!(
            version(at(0), fresh_for(60)),
            version(at(0), Freshness::Stale)
        );
        assert_ne!(version(at(0), fresh_for(60)), version(at(1), fresh_for(60)));
    }

    fn view(as_of: Timestamp) -> BoardView {
        let train = |arrival_at| TrainArrival {
            sequence: 1,
            destination: dut_core::station!("POA"),
            platforms: Platforms::one(1),
            arrival_at,
            time_type: None,
            via_racecourse: false,
        };
        let board = NextTrainBoard {
            line: Line::TseungKwanO,
            station: dut_core::station!("TKO"),
            generated_at: at(0),
            delayed: false,
            alert: None,
            trains: ByDirection::new(vec![train(at(100)), train(at(200))], Vec::new()),
        };
        BoardView::new(Snapshot::new(Arc::new(board), at(0), fresh_for(60)), as_of)
    }

    #[test]
    fn a_board_body_changes_only_when_a_train_departs() {
        let departed = |seconds| at(seconds) + DEPARTED_GRACE + SignedDuration::from_secs(1);

        assert_eq!(
            BoardVersion::of(&view(at(0))),
            BoardVersion::of(&view(at(100)))
        );
        assert_ne!(
            BoardVersion::of(&view(at(100))),
            BoardVersion::of(&view(departed(100)))
        );
        assert_eq!(
            BoardVersion::of(&view(departed(100))),
            BoardVersion::of(&view(at(200)))
        );
    }
}
