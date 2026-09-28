use serde::{Serialize, Serializer};

use dut_core::{
    application::next_train::{BoardView, LineBoard, StationBoards},
    domain::{
        network::Direction,
        next_train::{AlertNotice, TimeType, TrainArrival},
    },
};

use crate::dto::{
    ErrorDetail,
    common::{DirectionCode, HktTime, LineRef, StationRef},
};

/// `GET /api/lines/{line}/stations/{station}/next-trains`
#[derive(Debug, Serialize)]
pub(crate) struct NextTrainResponse<'a> {
    pub line: LineRef,
    pub station: StationRef<'a>,
    #[serde(flatten)]
    pub board: BoardBody<'a>,
}

impl<'a> From<&'a BoardView> for NextTrainResponse<'a> {
    fn from(view: &'a BoardView) -> Self {
        let board = view.board();
        Self {
            line: board.line.into(),
            station: (&board.station).into(),
            board: view.into(),
        }
    }
}

/// `GET /api/stations/{station}/next-trains`
#[derive(Debug, Serialize)]
pub(crate) struct StationBoardsResponse<'a> {
    pub station: StationRef<'a>,
    pub lines: Vec<LineBoardBody<'a>>,
}

/// One line at a station: either `board` or `error` is set.
#[derive(Debug, Serialize)]
pub(crate) struct LineBoardBody<'a> {
    pub line: LineRef,
    pub board: Option<BoardBody<'a>>,
    pub error: Option<ErrorDetail>,
}

impl<'a> StationBoardsResponse<'a> {
    /// Builds the response; `unavailable` describes lines that failed.
    pub(crate) fn new(boards: &'a StationBoards, unavailable: &ErrorDetail) -> Self {
        let lines = boards
            .lines()
            .iter()
            .map(|LineBoard { line, board }| LineBoardBody {
                line: (*line).into(),
                board: board.as_ref().ok().map(BoardBody::from),
                error: board.is_err().then(|| unavailable.clone()),
            })
            .collect();

        Self {
            station: boards.station().into(),
            lines,
        }
    }
}

/// The part of a board that is shared by single-line and station responses.
#[derive(Debug, Serialize)]
pub(crate) struct BoardBody<'a> {
    /// When the MTR generated the data.
    pub generated_at: HktTime,
    /// When this service fetched it.
    pub fetched_at: HktTime,
    /// Whether the data is past its freshness period because upstream failed.
    pub stale: bool,
    pub delayed: bool,
    pub alert: Option<AlertBody<'a>>,
    pub directions: Directions<'a>,
}

impl<'a> From<&'a BoardView> for BoardBody<'a> {
    fn from(view: &'a BoardView) -> Self {
        let board = view.board();
        let snapshot = view.snapshot();
        Self {
            generated_at: HktTime(board.generated_at),
            fetched_at: HktTime(snapshot.fetched_at()),
            stale: snapshot.freshness().is_stale(),
            delayed: board.delayed,
            alert: board.alert.as_ref().map(|alert| AlertBody {
                en: (&alert.en).into(),
                tc: (&alert.tc).into(),
            }),
            directions: Directions(view),
        }
    }
}

/// The directions a rider can board in, serialized straight from the cached
/// board without an intermediate collection.
#[derive(Debug)]
pub(crate) struct Directions<'a>(&'a BoardView);

impl Serialize for Directions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let view = self.0;
        serializer.collect_seq(view.directions().map(|direction| DirectionBody {
            direction: DirectionCode(direction),
            towards: Towards { view, direction },
            trains: Upcoming { view, direction },
        }))
    }
}

/// One direction at a station: where it heads and the trains still to come.
#[derive(Debug, Serialize)]
pub(super) struct DirectionBody<'a> {
    pub direction: DirectionCode,
    /// The termini beyond this station, as platform signs name them.
    pub towards: Towards<'a>,
    pub trains: Upcoming<'a>,
}

/// The termini a direction leads to from the board's station.
#[derive(Debug)]
pub(super) struct Towards<'a> {
    view: &'a BoardView,
    direction: Direction,
}

impl Serialize for Towards<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let board = self.view.board();
        serializer.collect_seq(
            board
                .line
                .towards(board.station, self.direction)
                .map(StationRef::from),
        )
    }
}

/// The trains still to come in one direction.
#[derive(Debug)]
pub(super) struct Upcoming<'a> {
    view: &'a BoardView,
    direction: Direction,
}

impl Serialize for Upcoming<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.view.upcoming(self.direction).map(TrainBody::from))
    }
}

/// One upcoming train. Trains are listed in the order they are due, so no
/// separate sequence number is exposed.
#[derive(Debug, Serialize)]
pub(super) struct TrainBody<'a> {
    pub destination: StationRef<'a>,
    pub platform: u8,
    /// Estimated arrival, or departure when `time_type` is `departure`.
    /// Clients derive countdowns from this absolute time.
    pub arrival_at: HktTime,
    /// `arrival` or `departure`; East Rail Line only.
    pub time_type: Option<&'static str>,
    /// East Rail Line trains running via Racecourse.
    pub via_racecourse: bool,
}

impl<'a> From<&'a TrainArrival> for TrainBody<'a> {
    fn from(train: &'a TrainArrival) -> Self {
        Self {
            destination: (&train.destination).into(),
            platform: train.platform,
            arrival_at: HktTime(train.arrival_at),
            time_type: train.time_type.map(|time_type| match time_type {
                TimeType::Arrival => "arrival",
                TimeType::Departure => "departure",
            }),
            via_racecourse: train.via_racecourse,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct AlertBody<'a> {
    pub en: NoticeBody<'a>,
    pub tc: NoticeBody<'a>,
}

#[derive(Debug, Serialize)]
pub(crate) struct NoticeBody<'a> {
    pub message: &'a str,
    pub url: Option<&'a str>,
}

impl<'a> From<&'a AlertNotice> for NoticeBody<'a> {
    fn from(notice: &'a AlertNotice) -> Self {
        Self {
            message: &notice.message,
            url: notice.url.as_deref(),
        }
    }
}
