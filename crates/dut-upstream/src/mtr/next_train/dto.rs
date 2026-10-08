//! Wire format of the Next Train API (`getSchedule.php`).
//!
//! The shapes follow what the API actually returns, which differs from the
//! data dictionary in places: numbers such as `seq` and `plat` arrive as
//! strings, `plat` is two numbers such as `1/3` at Airport, the East Rail Line
//! field is spelled `timeType`, and errors use a separate
//! `{resultCode, error: {errorCode, errorMsg}}` envelope.

use std::{borrow::Cow, fmt};

use jiff::Timestamp;
use serde::{
    Deserialize, Deserializer,
    de::{self, IgnoredAny, MapAccess, SeqAccess, Unexpected, Visitor},
};
use thiserror::Error;
use tracing::warn;

use dut_core::domain::{
    network::{ByDirection, Line, StationCode},
    next_train::{AlertNotice, NextTrainBoard, Platforms, TimeType, TrainArrival},
};

use crate::mtr::time::parse_local;

/// The error code the API uses when it has no trains to report.
const NO_CONTENT_CODE: &str = "NT-204";

/// A response, borrowing its text from the body it was decoded from.
#[derive(Debug, Deserialize)]
pub(super) struct ScheduleResponse<'a> {
    #[serde(deserialize_with = "lenient_number")]
    status: u8,
    #[serde(default, borrow)]
    message: Option<Text<'a>>,
    /// Link to a special arrangement notice.
    #[serde(default, borrow)]
    url: Option<Text<'a>>,
    #[serde(default, borrow)]
    isdelay: Option<Text<'a>>,
    #[serde(default, borrow)]
    curr_time: Option<Text<'a>>,
    /// Only present in error responses.
    #[serde(default, borrow)]
    timestamp: Option<Text<'a>>,
    #[serde(default, borrow)]
    error: Option<ErrorBody<'a>>,
    #[serde(default, borrow)]
    data: Option<Stations<'a>>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody<'a> {
    #[serde(rename = "errorCode", borrow)]
    code: Text<'a>,
    #[serde(rename = "errorMsg", default, borrow)]
    message: Option<Text<'a>>,
}

/// The `data` object, keyed by `"{line}-{station}"`. It holds the one
/// station asked for, so its entries are kept in order rather than hashed.
#[derive(Debug, Default)]
struct Stations<'a>(Vec<(Text<'a>, StationSchedule<'a>)>);

impl<'a> Stations<'a> {
    fn remove(self, line: Line, station: StationCode) -> Option<StationSchedule<'a>> {
        let wanted = Some((line.code(), station.as_str()));
        self.0
            .into_iter()
            .find(|(key, _)| key.as_str().split_once('-') == wanted)
            .map(|(_, schedule)| schedule)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Stations<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StationsVisitor;

        impl<'de> Visitor<'de> for StationsVisitor {
            type Value = Stations<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object of station schedules")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(1));
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Stations(entries))
            }
        }

        deserializer.deserialize_map(StationsVisitor)
    }
}

#[derive(Debug, Deserialize)]
struct StationSchedule<'a> {
    #[serde(default, borrow)]
    curr_time: Option<Text<'a>>,
    #[serde(rename = "UP", default, borrow)]
    up: Vec<TrainEntry<'a>>,
    #[serde(rename = "DOWN", default, borrow)]
    down: Vec<TrainEntry<'a>>,
}

/// One train. `ttnt`, `valid`, and `source` are ignored: the data dictionary
/// marks them as dummies, and `ttnt` is a countdown that goes stale in cache.
#[derive(Debug, Deserialize)]
struct TrainEntry<'a> {
    #[serde(deserialize_with = "lenient_number")]
    seq: u8,
    #[serde(borrow)]
    dest: Text<'a>,
    /// Read leniently, so that a platform the API garbles leaves the train
    /// listed without one instead of failing the whole board.
    #[serde(default, borrow)]
    plat: Option<RawPlatforms<'a>>,
    #[serde(borrow)]
    time: Text<'a>,
    #[serde(rename = "timeType", alias = "timetype", default, borrow)]
    time_type: Option<Text<'a>>,
    #[serde(default, borrow)]
    route: Option<Text<'a>>,
}

/// A train's `plat` as the API sent it: usually a string such as `"1"`, or
/// `"1/3"` at Airport, occasionally a number.
#[derive(Debug)]
enum RawPlatforms<'a> {
    Read(Platforms),
    /// Text that is not one or two platform numbers, kept for the log.
    Unreadable(Text<'a>),
    /// Neither text nor a platform number.
    Other,
}

impl RawPlatforms<'_> {
    const fn platforms(&self) -> Option<Platforms> {
        match self {
            Self::Read(platforms) => Some(*platforms),
            Self::Unreadable(_) | Self::Other => None,
        }
    }

    /// What the API sent, for the log.
    fn sent(&self) -> &str {
        match self {
            Self::Read(_) => "platform numbers",
            Self::Unreadable(text) => text.as_str(),
            Self::Other => "neither text nor a platform number",
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for RawPlatforms<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PlatformsVisitor;

        impl PlatformsVisitor {
            fn text(text: Cow<'_, str>) -> RawPlatforms<'_> {
                match text.parse() {
                    Ok(platforms) => RawPlatforms::Read(platforms),
                    Err(_) => RawPlatforms::Unreadable(Text(text)),
                }
            }
        }

        impl<'de> Visitor<'de> for PlatformsVisitor {
            type Value = RawPlatforms<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("platform numbers")
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(u8::try_from(value).map_or(RawPlatforms::Other, |number| {
                    RawPlatforms::Read(Platforms::one(number))
                }))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(u8::try_from(value).map_or(RawPlatforms::Other, |number| {
                    RawPlatforms::Read(Platforms::one(number))
                }))
            }

            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(RawPlatforms::Other)
            }

            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(RawPlatforms::Other)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(RawPlatforms::Other)
            }

            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Self::text(Cow::Borrowed(value)))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Self::text(Cow::Owned(value.to_owned())))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                IgnoredAny.visit_seq(seq).map(|_| RawPlatforms::Other)
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                IgnoredAny.visit_map(map).map(|_| RawPlatforms::Other)
            }
        }

        deserializer.deserialize_any(PlatformsVisitor)
    }
}

/// A string from the response, borrowed from the body unless the JSON
/// escaped a character in it, so decoding a board copies almost no text.
#[derive(Debug)]
struct Text<'a>(Cow<'a, str>);

impl Text<'_> {
    fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0.into_owned()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Text<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TextVisitor;

        impl<'de> Visitor<'de> for TextVisitor {
            type Value = Text<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Borrowed(value)))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(value.to_owned())))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(value)))
            }
        }

        deserializer.deserialize_str(TextVisitor)
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

impl ScheduleResponse<'_> {
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
            if error.code.as_str() != NO_CONTENT_CODE {
                return Ok(Schedule::Rejected {
                    code: error.code.into_string(),
                    message: error.message.map(Text::into_string).unwrap_or_default(),
                });
            }
            let generated_at = parse_optional_time(self.timestamp.as_ref())?.unwrap_or(fetched_at);
            return Ok(Schedule::Published {
                board: empty_board(line, station, generated_at),
                notice: None,
            });
        }

        let schedule = self
            .data
            .and_then(|stations| stations.remove(line, station));
        let curr_time = schedule
            .as_ref()
            .and_then(|schedule| schedule.curr_time.as_ref())
            .or(self.curr_time.as_ref());
        let generated_at = parse_optional_time(curr_time)?.unwrap_or(fetched_at);

        let mut board = empty_board(line, station, generated_at);
        board.delayed = is(self.isdelay.as_ref(), "Y");
        if let Some(schedule) = schedule {
            board.trains = ByDirection::new(trains(schedule.up)?, trains(schedule.down)?);
        }

        let notice = (self.status == 0).then(|| AlertNotice {
            message: self.message.map(Text::into_string).unwrap_or_default(),
            url: self
                .url
                .filter(|url| !url.as_str().trim().is_empty())
                .map(Text::into_string),
        });

        Ok(Schedule::Published { board, notice })
    }
}

impl TrainEntry<'_> {
    fn into_arrival(self) -> Result<TrainArrival, ScheduleDataError> {
        let destination = self
            .dest
            .as_str()
            .parse()
            .map_err(|_| ScheduleDataError::StationCode(self.dest.as_str().to_owned()))?;
        let time_type = match self.time_type.as_ref().map(Text::as_str) {
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
                    plat = ?self.plat.as_ref().map(RawPlatforms::sent),
                    destination = %destination,
                    "Next Train API published an unreadable platform; listing the train without one"
                );
                Platforms::NONE
            });

        Ok(TrainArrival {
            sequence: self.seq,
            destination,
            platforms,
            arrival_at: parse_time(self.time.as_str())?,
            time_type,
            via_racecourse: is(self.route.as_ref(), "RAC"),
        })
    }
}

fn trains(entries: Vec<TrainEntry<'_>>) -> Result<Vec<TrainArrival>, ScheduleDataError> {
    let mut trains = entries
        .into_iter()
        .map(TrainEntry::into_arrival)
        .collect::<Result<Vec<_>, _>>()?;
    trains.sort_unstable_by_key(|train| train.sequence);
    Ok(trains)
}

fn is(text: Option<&Text<'_>>, expected: &str) -> bool {
    text.is_some_and(|text| text.as_str() == expected)
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

fn parse_optional_time(value: Option<&Text<'_>>) -> Result<Option<Timestamp>, ScheduleDataError> {
    value
        .map(Text::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(parse_time)
        .transpose()
}

/// Accepts a small unsigned number written either as a JSON number or as a
/// string, since the API uses both.
fn lenient_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    struct LenientNumber;

    impl Visitor<'_> for LenientNumber {
        type Value = u8;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a number from 0 to 255, possibly written as text")
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<u8, E> {
            u8::try_from(value).map_err(|_| E::invalid_value(Unexpected::Unsigned(value), &self))
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<u8, E> {
            u8::try_from(value).map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<u8, E> {
            value
                .trim()
                .parse()
                .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
        }
    }

    deserializer.deserialize_any(LenientNumber)
}

#[cfg(test)]
mod tests {
    use dut_core::domain::network::Direction;
    use dut_core::station;

    use super::*;
    use crate::fixtures;

    /// Decodes and converts a captured response, which borrows from the
    /// body and so cannot outlive this call.
    fn convert(
        name: &str,
        line: Line,
        station: StationCode,
    ) -> Result<Schedule, ScheduleDataError> {
        let body = fixtures::read(&format!("mtr/{name}"));
        let response: ScheduleResponse<'_> =
            serde_json::from_slice(&body).expect("fixture should decode");
        response.into_schedule(line, station, Timestamp::UNIX_EPOCH)
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
        let schedule = convert(
            "next_train_tkl_tko.json",
            Line::TseungKwanO,
            station!("TKO"),
        )
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
        let schedule = convert("next_train_eal_taw.json", Line::EastRail, station!("TAW"))
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
        let schedule = convert(
            "next_train_ael_air.json",
            Line::AirportExpress,
            station!("AIR"),
        )
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
        let schedule = convert("next_train_empty.json", Line::TseungKwanO, station!("TKO"))
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
    fn decodes_text_the_json_escapes() {
        let response: ScheduleResponse = serde_json::from_str(
            r#"{"status":0,"message":"Trains \u7d93 Racecourse","url":"https:\/\/example.com\/notice","curr_time":"2026-09-27 22:35:36","data":{}}"#,
        )
        .expect("escaped response should decode");

        let schedule = response
            .into_schedule(Line::EastRail, station!("TAW"), Timestamp::UNIX_EPOCH)
            .expect("alert should convert");
        let (_, notice) = published(schedule);

        assert_eq!(
            notice,
            Some(AlertNotice {
                message: "Trains 經 Racecourse".to_owned(),
                url: Some("https://example.com/notice".to_owned()),
            })
        );
    }

    #[test]
    fn ignores_schedules_for_other_stations() {
        let response: ScheduleResponse = serde_json::from_str(
            r#"{"status":1,"curr_time":"2026-09-27 22:35:36","data":{"TKL-POA":{"UP":[
                {"seq":"1","dest":"POA","plat":"1","time":"2026-09-27 22:36:36"}
            ]}}}"#,
        )
        .expect("response should decode");

        let schedule = response
            .into_schedule(Line::TseungKwanO, station!("TKO"), Timestamp::UNIX_EPOCH)
            .expect("board should convert");
        let (board, _) = published(schedule);

        assert!(board.trains.up.is_empty() && board.trains.down.is_empty());
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
