//! Wire format of the Observatory's weather warning information
//! (`weather.php?dataType=warningInfo`).
//!
//! This document is read rather than the shorter warning summary
//! (`warnsum`) because only it lists the pre-No. 8 special announcement.

use jiff::Timestamp;
use serde::Deserialize;
use tracing::{debug, warn};

use dut_core::domain::weather::{
    ActiveWarning, CycloneSignal, FireDangerLevel, Quadrant, RainstormLevel, WeatherWarning,
    WeatherWarnings,
};

/// The whole document. `details` is absent when no warning is in force.
#[derive(Debug, Deserialize)]
pub(super) struct WarningInfoResponse {
    #[serde(default)]
    details: Vec<WarningDetail>,
}

/// One warning. Its `contents` text is not read.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WarningDetail {
    warning_statement_code: String,
    /// The level, for the warnings that have one, such as `TC8NE`.
    #[serde(default)]
    subtype: Option<String>,
    /// RFC 3339 with a `+08:00` offset. May be absent.
    #[serde(default)]
    update_time: Option<String>,
}

impl WarningInfoResponse {
    /// Converts the document into domain terms. A cancelled signal that is
    /// still listed is left out, since it is no longer in force.
    pub(super) fn into_warnings(self) -> WeatherWarnings {
        WeatherWarnings {
            warnings: self
                .details
                .into_iter()
                .filter_map(WarningDetail::into_active_warning)
                .collect(),
        }
    }
}

impl WarningDetail {
    fn into_active_warning(self) -> Option<ActiveWarning> {
        let code = self.warning_statement_code.trim();
        let subtype = self.subtype.as_deref().map(str::trim);
        if code == "WTCSGNL" && subtype == Some("CANCEL") {
            debug!("tropical cyclone signals are listed as cancelled");
            return None;
        }

        let warning = warning(code, subtype).unwrap_or_else(|| {
            let verbatim = match subtype {
                Some(subtype) => format!("{code}/{subtype}"),
                None => code.to_owned(),
            };
            warn!(code = verbatim, "unrecognised weather warning");
            WeatherWarning::Unrecognised(verbatim)
        });

        Some(ActiveWarning {
            warning,
            updated_at: self.update_time.as_deref().and_then(update_time),
        })
    }
}

/// Maps a statement code and its subtype to a warning, or `None` when either
/// is not one the Observatory documents.
fn warning(code: &str, subtype: Option<&str>) -> Option<WeatherWarning> {
    let warning = match code {
        "WTCSGNL" => WeatherWarning::TropicalCyclone(cyclone_signal(subtype?)?),
        "WTCPRE8" => WeatherWarning::PreNo8Announcement,
        "WRAIN" => WeatherWarning::Rainstorm(match subtype? {
            "WRAINA" => RainstormLevel::Amber,
            "WRAINR" => RainstormLevel::Red,
            "WRAINB" => RainstormLevel::Black,
            _ => return None,
        }),
        "WFIRE" => WeatherWarning::FireDanger(match subtype? {
            "WFIREY" => FireDangerLevel::Yellow,
            "WFIRER" => FireDangerLevel::Red,
            _ => return None,
        }),
        "WHOT" => WeatherWarning::HotWeather,
        "WCOLD" => WeatherWarning::ColdWeather,
        "WFROST" => WeatherWarning::Frost,
        "WMSGNL" => WeatherWarning::StrongMonsoon,
        "WFNTSA" => WeatherWarning::NorthernNtFlooding,
        "WL" => WeatherWarning::Landslip,
        "WTMW" => WeatherWarning::Tsunami,
        "WTS" => WeatherWarning::Thunderstorm,
        _ => return None,
    };
    Some(warning)
}

fn cyclone_signal(subtype: &str) -> Option<CycloneSignal> {
    let signal = match subtype {
        "TC1" => CycloneSignal::One,
        "TC3" => CycloneSignal::Three,
        "TC8NE" => CycloneSignal::Eight(Quadrant::NorthEast),
        "TC8NW" => CycloneSignal::Eight(Quadrant::NorthWest),
        "TC8SE" => CycloneSignal::Eight(Quadrant::SouthEast),
        "TC8SW" => CycloneSignal::Eight(Quadrant::SouthWest),
        "TC9" => CycloneSignal::Nine,
        "TC10" => CycloneSignal::Ten,
        _ => return None,
    };
    Some(signal)
}

/// A malformed time is logged and dropped rather than failing the document:
/// the warning itself is still worth knowing about.
fn update_time(value: &str) -> Option<Timestamp> {
    value
        .parse()
        .inspect_err(|parse_error| {
            warn!(
                value,
                error = parse_error as &dyn std::error::Error,
                "invalid weather warning update time"
            );
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn warnings(json: &str) -> Vec<ActiveWarning> {
        serde_json::from_str::<WarningInfoResponse>(json)
            .expect("document should decode")
            .into_warnings()
            .warnings
    }

    fn kinds(json: &str) -> Vec<WeatherWarning> {
        warnings(json)
            .into_iter()
            .map(|active| active.warning)
            .collect()
    }

    #[test]
    fn decodes_the_live_document() {
        let body = fixtures::read("hko/warning_info.json");
        let response: WarningInfoResponse =
            serde_json::from_slice(&body).expect("fixture should decode");

        let warnings = response.into_warnings().warnings;

        assert_eq!(
            warnings,
            [ActiveWarning {
                warning: WeatherWarning::HotWeather,
                updated_at: Some("2026-09-28T12:15:00Z".parse().expect("valid timestamp")),
            }]
        );
    }

    #[test]
    fn an_empty_document_means_no_warnings() {
        assert!(warnings("{}").is_empty());
        assert!(warnings(r#"{"details":[]}"#).is_empty());
    }

    #[test]
    fn reads_levels_from_the_subtype() {
        let decoded = kinds(
            r#"{"details":[
                {"warningStatementCode":"WTCSGNL","subtype":"TC8NE","updateTime":"2026-09-28T20:15:00+08:00"},
                {"warningStatementCode":"WRAIN","subtype":"WRAINB"},
                {"warningStatementCode":"WFIRE","subtype":"WFIREY"}
            ]}"#,
        );

        assert_eq!(
            decoded,
            [
                WeatherWarning::TropicalCyclone(CycloneSignal::Eight(Quadrant::NorthEast)),
                WeatherWarning::Rainstorm(RainstormLevel::Black),
                WeatherWarning::FireDanger(FireDangerLevel::Yellow),
            ]
        );
    }

    #[test]
    fn reads_the_pre_no_8_announcement() {
        assert_eq!(
            kinds(r#"{"details":[{"warningStatementCode":"WTCPRE8"}]}"#),
            [WeatherWarning::PreNo8Announcement]
        );
    }

    #[test]
    fn leaves_out_cancelled_cyclone_signals() {
        assert!(
            kinds(r#"{"details":[{"warningStatementCode":"WTCSGNL","subtype":"CANCEL"}]}"#)
                .is_empty()
        );
    }

    #[test]
    fn keeps_unrecognised_warnings_verbatim() {
        let decoded = kinds(
            r#"{"details":[
                {"warningStatementCode":"WNEW"},
                {"warningStatementCode":"WTCSGNL","subtype":"TC11"}
            ]}"#,
        );

        assert_eq!(
            decoded,
            [
                WeatherWarning::Unrecognised("WNEW".to_owned()),
                WeatherWarning::Unrecognised("WTCSGNL/TC11".to_owned()),
            ]
        );
    }

    #[test]
    fn keeps_a_warning_whose_update_time_is_malformed() {
        let decoded =
            warnings(r#"{"details":[{"warningStatementCode":"WL","updateTime":"soon"}]}"#);

        assert_eq!(
            decoded,
            [ActiveWarning {
                warning: WeatherWarning::Landslip,
                updated_at: None,
            }]
        );
    }
}
