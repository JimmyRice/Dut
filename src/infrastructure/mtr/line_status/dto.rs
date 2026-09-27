//! Wire format of the MTR line status feed (`ryg_line_status.json`).

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tracing::{info, warn};

use crate::{
    domain::{
        line_status::{LineCondition, LineStatus, NetworkStatus},
        network::Line,
    },
    infrastructure::mtr::time::parse_local,
};

#[derive(Debug, Deserialize)]
pub(super) struct LineStatusFeed {
    ryg_status: RygStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RygStatus {
    /// Hong Kong local time. The XML variant of this feed labels the same
    /// value "GMT", but it is local time.
    last_build_date: String,
    #[serde(default)]
    line: Vec<LineEntry>,
}

/// One line. Names and colours are ignored in favour of the static network.
#[derive(Debug, Deserialize)]
struct LineEntry {
    line_code: String,
    status: String,
    /// Empty string when there is nothing to report. Its shape during
    /// incidents is not documented, so anything else is logged and skipped.
    #[serde(default)]
    messages: Value,
}

#[derive(Debug, Error)]
#[error("invalid lastBuildDate {value:?}")]
pub(super) struct LineStatusDataError {
    value: String,
    #[source]
    cause: jiff::Error,
}

impl LineStatusFeed {
    /// Converts the feed into domain terms. Lines this service does not know
    /// are skipped with a warning rather than failing the whole feed.
    pub(super) fn into_network_status(self) -> Result<NetworkStatus, LineStatusDataError> {
        let RygStatus {
            last_build_date,
            line,
        } = self.ryg_status;
        let updated_at = parse_local(&last_build_date).map_err(|cause| LineStatusDataError {
            value: last_build_date,
            cause,
        })?;
        let lines = line
            .into_iter()
            .filter_map(LineEntry::into_line_status)
            .collect();

        Ok(NetworkStatus { updated_at, lines })
    }
}

impl LineEntry {
    fn into_line_status(self) -> Option<LineStatus> {
        let Ok(line) = self.line_code.parse::<Line>() else {
            warn!(
                line_code = self.line_code,
                "skipping unknown line in line status feed"
            );
            return None;
        };

        let condition = condition(&self.status);
        if let LineCondition::Unknown(code) = &condition {
            warn!(%line, status = code, "unknown line status code");
        }

        let message = match self.messages {
            Value::String(text) if !text.trim().is_empty() => Some(text),
            Value::Null | Value::String(_) => None,
            other => {
                info!(%line, messages = %other, "line status messages in an unrecognised shape");
                None
            }
        };

        Some(LineStatus {
            line,
            condition,
            message,
        })
    }
}

fn condition(code: &str) -> LineCondition {
    match code.trim().to_ascii_lowercase().as_str() {
        "green" => LineCondition::Normal,
        "yellow" => LineCondition::Delayed,
        "red" => LineCondition::Disrupted,
        "pink" => LineCondition::DelayedOrDisrupted,
        "grey" | "gray" => LineCondition::NonServiceHours,
        "typhoon" => LineCondition::TyphoonSignal,
        _ => LineCondition::Unknown(code.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(json: &str) -> LineStatusFeed {
        serde_json::from_str(json).expect("feed should decode")
    }

    #[test]
    fn decodes_the_live_feed() {
        let path = format!(
            "{}/tests/fixtures/mtr/line_status.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let body = std::fs::read(path).expect("fixture should exist");
        let feed: LineStatusFeed = serde_json::from_slice(&body).expect("fixture should decode");

        let status = feed.into_network_status().expect("feed should convert");

        assert_eq!(status.updated_at.to_string(), "2026-09-26T22:15:00Z");
        assert_eq!(status.lines.len(), 11);
        assert_eq!(
            status.lines[0],
            LineStatus {
                line: Line::TsuenWan,
                condition: LineCondition::Normal,
                message: None,
            }
        );
        assert!(
            status
                .lines
                .iter()
                .any(|entry| entry.line == Line::LightRail)
        );
    }

    #[test]
    fn maps_every_documented_status_code() {
        let cases = [
            ("green", LineCondition::Normal),
            ("yellow", LineCondition::Delayed),
            ("red", LineCondition::Disrupted),
            ("pink", LineCondition::DelayedOrDisrupted),
            ("grey", LineCondition::NonServiceHours),
            ("typhoon", LineCondition::TyphoonSignal),
            ("Green", LineCondition::Normal),
            ("blue", LineCondition::Unknown("blue".to_owned())),
        ];

        for (code, expected) in cases {
            assert_eq!(condition(code), expected, "{code}");
        }
    }

    #[test]
    fn keeps_text_messages_and_skips_unknown_lines() {
        let status = feed(
            r#"{"ryg_status":{"lastBuildDate":"2026-09-27 08:00:00","line":[
                {"line_code":"KTL","status":"yellow","messages":"Train service is delayed"},
                {"line_code":"NOL","status":"green","messages":""},
                {"line_code":"ISL","status":"green","messages":{"unexpected":true}}
            ]}}"#,
        )
        .into_network_status()
        .expect("feed should convert");

        assert_eq!(
            status.lines,
            [
                LineStatus {
                    line: Line::KwunTong,
                    condition: LineCondition::Delayed,
                    message: Some("Train service is delayed".to_owned()),
                },
                LineStatus {
                    line: Line::Island,
                    condition: LineCondition::Normal,
                    message: None,
                },
            ]
        );
    }

    #[test]
    fn rejects_a_malformed_build_date() {
        let result =
            feed(r#"{"ryg_status":{"lastBuildDate":"yesterday","line":[]}}"#).into_network_status();

        assert!(result.is_err());
    }
}
