//! Wire format of the Next Train API (`getSchedule.php`).
//!
//! The shapes follow what the API actually returns, which differs from the
//! data dictionary in places: numbers such as `seq` and `plat` arrive as
//! strings, `plat` is two numbers such as `1/3` at Airport, the East Rail Line
//! field is spelled `timeType`, and errors use a separate
//! `{resultCode, error: {errorCode, errorMsg}}` envelope.

use std::collections::HashMap;

use jiff::Timestamp;
use serde::{Deserialize, Deserializer, de::IgnoredAny};
use thiserror::Error;
use tracing::warn;

use dut_core::domain::{
    network::{ByDirection, Line, StationCode},
    next_train::{AlertNotice, NextTrainBoard, Platforms, TimeType, TrainArrival},
};

use crate::mtr::time::parse_local;

/// The error code the API uses when it has no trains to report.
const NO_CONTENT_CODE: &str = "NT-204";

#[derive(Debug, Deserialize)]
pub(super) struct ScheduleResponse {
    #[serde(deserialize_with = "lenient_number")]
    status: u8,
    #[serde(default)]
    message: Option<String>,
    /// Link to a special arrangement notice.
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    isdelay: Option<String>,
    #[serde(default)]
    curr_time: Option<String>,
    /// Only present in error responses.
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    error: Option<ErrorBody>,
    /// Keyed by `"{line}-{station}"`.
    #[serde(default)]
    data: Option<HashMap<String, StationSchedule>>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(rename = "errorCode")]
    code: String,
    #[serde(rename = "errorMsg", default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct StationSchedule {
    #[serde(default)]
    curr_time: Option<String>,
    #[serde(rename = "UP", default)]
    up: Vec<TrainEntry>,
    #[serde(rename = "DOWN", default)]
    down: Vec<TrainEntry>,
}

/// One train. `ttnt`, `valid`, and `source` are ignored: the data dictionary
/// marks them as dummies, and `ttnt` is a countdown that goes stale in cache.
#[derive(Debug, Deserialize)]
struct TrainEntry {
    #[serde(deserialize_with = "lenient_number")]
    seq: u8,
    dest: String,
    /// Kept as sent, so that a platform the API garbles leaves the train
    /// listed without one instead of failing the whole board.
    #[serde(default)]
    plat: Option<RawPlatforms>,
    time: String,
    #[serde(rename = "timeType", alias = "timetype", default)]
    time_type: Option<String>,
    #[serde(default)]
    route: Option<String>,
}

/// A train's `plat` as the API sent it: usually a string such as `"1"`, or
/// `"1/3"` at Airport.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawPlatforms {
    Text(String),
    Number(u8),
    Other(IgnoredAny),
}

impl RawPlatforms {
    fn platforms(&self) -> Option<Platforms> {
        match self {
            Self::Text(text) => text.parse().ok(),
            Self::Number(number) => Some(Platforms::one(*number)),
            Self::Other(_) => None,
        }
    }
}

/// What the API said about one station.
#[derive(Debug)]
pub(super) enum Schedule {
    /// A board, possibly empty. `notice` is set when the API flagged a special
    /// arrangement; it is in the language of the request.
    Published {
        board: NextTrainBoard,
        notice: Option<AlertNotice>,
    },
    /// The API refused the request.
    Rejected { code: String, message: String },
}

#[derive(Debug, Error)]
pub(super) enum ScheduleDataError {
    #[error("invalid time {value:?}")]
    Time {
        value: String,
        #[source]
        cause: jiff::Error,
    },

    #[error("invalid station code {0:?}")]
    StationCode(String),
}

impl ScheduleResponse {
    /// Converts the response for `line` and `station` into domain terms.
    ///
    /// `fetched_at` stands in for the generation time when the API omits it.
    pub(super) fn into_schedule(
        self,
        line: Line,
        station: StationCode,
        fetched_at: Timestamp,
    ) -> Result<Schedule, ScheduleDataError> {
        if let Some(error) = self.error {
            if error.code != NO_CONTENT_CODE {
                return Ok(Schedule::Rejected {
                    code: error.code,
                    message: error.message,
                });
            }
            let generated_at =
                parse_optional_time(self.timestamp.as_deref())?.unwrap_or(fetched_at);
            return Ok(Schedule::Published {
                board: empty_board(line, station, generated_at),
                notice: None,
            });
        }

        let mut data = self.data.unwrap_or_default();
        let schedule = data.remove(&format!("{line}-{station}"));
        let curr_time = schedule
            .as_ref()
            .and_then(|schedule| schedule.curr_time.as_deref())
            .or(self.curr_time.as_deref());
        let generated_at = parse_optional_time(curr_time)?.unwrap_or(fetched_at);

        let mut board = empty_board(line, station, generated_at);
        board.delayed = self.isdelay.as_deref() == Some("Y");
        if let Some(schedule) = schedule {
            board.trains = ByDirection::new(trains(schedule.up)?, trains(schedule.down)?);
        }

        let notice = (self.status == 0).then(|| AlertNotice {
            message: self.message.unwrap_or_default(),
            url: self.url.filter(|url| !url.trim().is_empty()),
        });

        Ok(Schedule::Published { board, notice })
    }
}

impl TrainEntry {
    fn into_arrival(self) -> Result<TrainArrival, ScheduleDataError> {
        let destination = self
            .dest
            .parse()
            .map_err(|_| ScheduleDataError::StationCode(self.dest.clone()))?;
        let time_type = match self.time_type.as_deref() {
            Some("A") => Some(TimeType::Arrival),
            Some("D") => Some(TimeType::Departure),
            _ => None,
        };
        let platforms = self
            .plat
            .as_ref()
            .and_then(RawPlatforms::platforms)
            .unwrap_or_else(|| {
                warn!(
                    plat = ?self.plat,
                    destination = %destination,
                    "Next Train API published an unreadable platform; listing the train without one"
                );
                Platforms::NONE
            });

        Ok(TrainArrival {
            sequence: self.seq,
            destination,
            platforms,
            arrival_at: parse_time(&self.time)?,
            time_type,
            via_racecourse: self.route.as_deref() == Some("RAC"),
        })
    }
}

fn trains(entries: Vec<TrainEntry>) -> Result<Vec<TrainArrival>, ScheduleDataError> {
    let mut trains = entries
        .into_iter()
        .map(TrainEntry::into_arrival)
        .collect::<Result<Vec<_>, _>>()?;
    trains.sort_by_key(|train| train.sequence);
    Ok(trains)
}

fn empty_board(line: Line, station: StationCode, generated_at: Timestamp) -> NextTrainBoard {
    NextTrainBoard {
        line,
        station,
        generated_at,
        delayed: false,
        alert: None,
        trains: ByDirection::default(),
    }
}

fn parse_time(value: &str) -> Result<Timestamp, ScheduleDataError> {
    parse_local(value).map_err(|cause| ScheduleDataError::Time {
        value: value.to_owned(),
        cause,
    })
}

fn parse_optional_time(value: Option<&str>) -> Result<Option<Timestamp>, ScheduleDataError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(parse_time)
        .transpose()
}

/// Accepts a small unsigned number written either as a JSON number or as a
/// string, since the API uses both.
fn lenient_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrText {
        Number(u8),
        Text(String),
    }

    match NumberOrText::deserialize(deserializer)? {
        NumberOrText::Number(number) => Ok(number),
        NumberOrText::Text(text) => text.trim().parse().map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use dut_core::domain::network::Direction;
    use dut_core::station;

    use super::*;
    use crate::fixtures;

    fn fixture(name: &str) -> ScheduleResponse {
        let body = fixtures::read(&format!("mtr/{name}"));
        serde_json::from_slice(&body).expect("fixture should decode")
    }

    fn published(schedule: Schedule) -> (NextTrainBoard, Option<AlertNotice>) {
        match schedule {
            Schedule::Published { board, notice } => (board, notice),
            Schedule::Rejected { code, .. } => panic!("unexpected rejection {code}"),
        }
    }

    fn platforms(board: &NextTrainBoard, direction: Direction) -> Vec<Platforms> {
        board
            .trains
            .get(direction)
            .iter()
            .map(|train| train.platforms)
            .collect()
    }

    #[test]
    fn decodes_a_regular_board() {
        let schedule = fixture("next_train_tkl_tko.json")
            .into_schedule(Line::TseungKwanO, station!("TKO"), Timestamp::UNIX_EPOCH)
            .expect("board should convert");
        let (board, notice) = published(schedule);

        assert_eq!(
            board.generated_at,
            parse_local("2026-09-27 22:35:36").expect("valid time")
        );
        assert!(!board.delayed);
        assert!(notice.is_none());

        let up = board.trains.get(Direction::Up);
        assert_eq!(up.len(), 4);
        assert_eq!(
            up[0],
            TrainArrival {
                sequence: 1,
                destination: station!("LHP"),
                platforms: Platforms::one(1),
                arrival_at: parse_local("2026-09-27 22:36:36").expect("valid time"),
                time_type: None,
                via_racecourse: false,
            }
        );
        assert_eq!(board.trains.get(Direction::Down).len(), 4);
    }

    #[test]
    fn decodes_east_rail_line_extensions() {
        let schedule = fixture("next_train_eal_taw.json")
            .into_schedule(Line::EastRail, station!("TAW"), Timestamp::UNIX_EPOCH)
            .expect("board should convert");
        let (board, _) = published(schedule);

        assert!(board.delayed);
        let up = board.trains.get(Direction::Up);
        assert_eq!(up[0].time_type, Some(TimeType::Arrival));
        assert!(!up[0].via_racecourse);
        assert_eq!(up[1].time_type, Some(TimeType::Departure));
        assert!(up[1].via_racecourse);
    }

    #[test]
    fn decodes_both_platforms_at_airport() {
        let schedule = fixture("next_train_ael_air.json")
            .into_schedule(Line::AirportExpress, station!("AIR"), Timestamp::UNIX_EPOCH)
            .expect("board should convert");
        let (board, _) = published(schedule);

        assert_eq!(platforms(&board, Direction::Up), [Platforms::pair(1, 3); 4]);
        assert_eq!(
            platforms(&board, Direction::Down),
            [Platforms::pair(2, 4); 4]
        );
    }

    #[test]
    fn keeps_a_train_whose_platform_is_unreadable() {
        let response: ScheduleResponse = serde_json::from_str(
            r#"{"status":1,"curr_time":"2026-10-02 16:11:15","data":{"AEL-AIR":{"UP":[
                {"seq":"1","dest":"AWE","plat":"","time":"2026-10-02 16:12:00"},
                {"seq":"2","dest":"AWE","plat":"1/2/3","time":"2026-10-02 16:23:00"},
                {"seq":"3","dest":"AWE","plat":null,"time":"2026-10-02 16:33:00"},
                {"seq":"4","dest":"AWE","plat":[1],"time":"2026-10-02 16:41:00"}
            ],"DOWN":[
                {"seq":"1","dest":"HOK","time":"2026-10-02 16:15:00"},
                {"seq":"2","dest":"HOK","plat":"2/4","time":"2026-10-02 16:26:00"}
            ]}}}"#,
        )
        .expect("response should decode");

        let schedule = response
            .into_schedule(Line::AirportExpress, station!("AIR"), Timestamp::UNIX_EPOCH)
            .expect("board should convert");
        let (board, _) = published(schedule);

        assert_eq!(platforms(&board, Direction::Up), [Platforms::NONE; 4]);
        assert_eq!(
            platforms(&board, Direction::Down),
            [Platforms::NONE, Platforms::pair(2, 4)]
        );
    }

    #[test]
    fn treats_no_content_as_an_empty_board() {
        let schedule = fixture("next_train_empty.json")
            .into_schedule(Line::TseungKwanO, station!("TKO"), Timestamp::UNIX_EPOCH)
            .expect("empty response should convert");
        let (board, notice) = published(schedule);

        assert_eq!(
            board.generated_at,
            parse_local("2026-09-27 22:35:56").expect("valid time")
        );
        assert!(board.trains.up.is_empty() && board.trains.down.is_empty());
        assert!(notice.is_none());
    }

    #[test]
    fn reports_other_errors_as_rejections() {
        let response: ScheduleResponse = serde_json::from_str(
            r#"{"resultCode":0,"error":{"errorCode":"NT-500","errorMsg":"Oops"},"status":0}"#,
        )
        .expect("error response should decode");

        let schedule = response
            .into_schedule(Line::TseungKwanO, station!("TKO"), Timestamp::UNIX_EPOCH)
            .expect("rejection should convert");

        assert!(matches!(schedule, Schedule::Rejected { code, .. } if code == "NT-500"));
    }

    #[test]
    fn surfaces_special_arrangement_alerts() {
        let response: ScheduleResponse = serde_json::from_str(
            r#"{"status":0,"message":"Special train service arrangement","url":"https://example.com/notice","curr_time":"2026-09-27 22:35:36","data":{}}"#,
        )
        .expect("alert response should decode");

        let schedule = response
            .into_schedule(Line::TseungKwanO, station!("TKO"), Timestamp::UNIX_EPOCH)
            .expect("alert should convert");
        let (_, notice) = published(schedule);

        assert_eq!(
            notice,
            Some(AlertNotice {
                message: "Special train service arrangement".to_owned(),
                url: Some("https://example.com/notice".to_owned()),
            })
        );
    }

    #[test]
    fn accepts_numbers_as_numbers_or_strings() {
        let entry = |json: &str| -> (u8, Platforms) {
            let entry: TrainEntry = serde_json::from_str(json).expect("entry should decode");
            let train = entry.into_arrival().expect("entry should convert");
            (train.sequence, train.platforms)
        };

        assert_eq!(
            entry(r#"{"seq":2,"dest":"POA","plat":"1","time":"2026-09-27 22:38:36"}"#),
            (2, Platforms::one(1))
        );
        assert_eq!(
            entry(r#"{"seq":"2","dest":"POA","plat":1,"time":"2026-09-27 22:38:36"}"#),
            (2, Platforms::one(1))
        );
    }

    #[test]
    fn rejects_malformed_times() {
        let response: ScheduleResponse =
            serde_json::from_str(r#"{"status":1,"data":{"TKL-TKO":{"curr_time":"soon","UP":[]}}}"#)
                .expect("response should decode");

        let result =
            response.into_schedule(Line::TseungKwanO, station!("TKO"), Timestamp::UNIX_EPOCH);

        assert!(matches!(result, Err(ScheduleDataError::Time { .. })));
    }
}
