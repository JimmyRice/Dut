use std::{fmt, time::Duration};

use jiff::Timestamp;
use reqwest::Url;
use thiserror::Error;
use tracing::{debug, info, warn};

use dut_core::{
    application::{
        next_train::NextTrainSource,
        source::{Snapshot, SourceUnavailable},
    },
    domain::{
        localized::Localized,
        network::{Direction, Line, StationCode},
        next_train::{AlertNotice, NextTrainBoard},
        time::HONG_KONG,
    },
};
use dut_http::{OutboundHttpClient, UpstreamError, UpstreamRequest, UpstreamResponse};
use dut_telemetry::millis;

use crate::{
    cache::{CachePolicy, Fetched, RefreshingCache},
    connectivity::Probe,
    mtr::next_train::dto::{Schedule, ScheduleDataError, ScheduleResponse},
};

const UPSTREAM: &str = "mtr.next_train";

/// The board the startup connectivity check asks for. Any board would do;
/// the API answers with JSON even when no trains are running.
const PROBE_BOARD: BoardKey = BoardKey {
    line: Line::TseungKwanO,
    station: StationCode::from_static("TKO"),
};

/// Next Train boards from the MTR open data API, cached per line and station.
///
/// Boards are fetched in English only: the payload is language-neutral station
/// codes, and names come from the static network. The Traditional Chinese
/// variant is requested only for the rare special arrangement notice.
#[derive(Debug)]
pub struct MtrNextTrainSource {
    client: NextTrainClient,
    cache: RefreshingCache<BoardKey, NextTrainBoard>,
}

impl MtrNextTrainSource {
    pub fn new(
        http: OutboundHttpClient,
        endpoint: Url,
        request_timeout: Duration,
        policy: CachePolicy,
    ) -> Self {
        Self {
            client: NextTrainClient {
                http,
                endpoint,
                request_timeout,
            },
            cache: RefreshingCache::new(UPSTREAM, policy),
        }
    }

    /// The request the startup connectivity check sends: one board in
    /// English, the same kind of request a rider's lookup makes.
    pub fn probe(&self) -> Probe {
        Probe::json(self.client.request(PROBE_BOARD, Language::English))
    }
}

impl NextTrainSource for MtrNextTrainSource {
    async fn board(
        &self,
        line: Line,
        station: StationCode,
    ) -> Result<Snapshot<NextTrainBoard>, SourceUnavailable> {
        let key = BoardKey { line, station };
        let client = self.client.clone();
        self.cache
            .get(&key, move || async move { client.fetch_board(key).await })
            .await
            .map_err(SourceUnavailable::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BoardKey {
    line: Line,
    station: StationCode,
}

impl fmt::Display for BoardKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.line, self.station)
    }
}

#[derive(Clone, Copy, Debug)]
enum Language {
    English,
    TraditionalChinese,
}

impl Language {
    const fn code(self) -> &'static str {
        match self {
            Self::English => "EN",
            Self::TraditionalChinese => "TC",
        }
    }
}

#[derive(Debug, Error)]
enum FetchError {
    #[error(transparent)]
    Upstream(#[from] UpstreamError),

    #[error("the Next Train API returned malformed JSON")]
    Json(#[from] serde_json::Error),

    #[error("the Next Train API returned unusable data")]
    Data(#[from] ScheduleDataError),

    #[error("the Next Train API rejected the request with {code}")]
    Rejected { code: String },
}

#[derive(Clone, Debug)]
struct NextTrainClient {
    http: OutboundHttpClient,
    endpoint: Url,
    request_timeout: Duration,
}

impl NextTrainClient {
    async fn fetch_board(&self, key: BoardKey) -> Result<Fetched<NextTrainBoard>, FetchError> {
        let (response, schedule) = self.fetch_schedule(key, Language::English).await?;
        let (mut board, notice) = match schedule {
            Schedule::Published { board, notice } => (board, notice),
            Schedule::Rejected { code, message } => {
                warn!(code, message, "Next Train API rejected the request");
                return Err(FetchError::Rejected { code });
            }
        };

        let upcoming = |direction| board.trains.get(direction).len();
        debug!(
            trains_up = upcoming(Direction::Up),
            trains_down = upcoming(Direction::Down),
            delayed = board.delayed,
            generated_at = %board.generated_at.display_with_offset(HONG_KONG),
            upstream_lag_ms = millis(Timestamp::now().duration_since(board.generated_at).unsigned_abs()),
            "decoded Next Train board"
        );
        if board.trains.up.is_empty() && board.trains.down.is_empty() {
            info!("Next Train API published no trains");
        }
        if board.delayed {
            info!("Next Train API reports delays");
        }

        if let Some(english) = notice {
            warn!(
                message = english.message,
                url = english.url,
                "Next Train API published a special arrangement notice"
            );
            board.alert = Some(self.localize_notice(key, english).await);
        }

        Ok(Fetched {
            value: board,
            ttl_hint: response.ttl_hint(),
        })
    }

    /// Completes an English notice with its Traditional Chinese counterpart,
    /// falling back to English if that cannot be fetched.
    async fn localize_notice(&self, key: BoardKey, english: AlertNotice) -> Localized<AlertNotice> {
        let chinese = match self.fetch_schedule(key, Language::TraditionalChinese).await {
            Ok((
                _,
                Schedule::Published {
                    notice: Some(notice),
                    ..
                },
            )) => Some(notice),
            Ok(_) => {
                warn!("Traditional Chinese response carried no notice; using English for both");
                None
            }
            Err(fetch_error) => {
                warn!(
                    error = &fetch_error as &dyn std::error::Error,
                    "could not fetch the Traditional Chinese notice; using English for both"
                );
                None
            }
        };
        let tc = chinese.unwrap_or_else(|| english.clone());
        Localized::new(english, tc)
    }

    fn request(&self, key: BoardKey, language: Language) -> UpstreamRequest {
        let mut url = self.endpoint.clone();
        url.query_pairs_mut()
            .append_pair("line", key.line.code())
            .append_pair("sta", key.station.as_str())
            .append_pair("lang", language.code());

        UpstreamRequest {
            upstream: UPSTREAM,
            url,
            timeout: self.request_timeout,
        }
    }

    async fn fetch_schedule(
        &self,
        key: BoardKey,
        language: Language,
    ) -> Result<(UpstreamResponse, Schedule), FetchError> {
        let response = self.http.fetch(self.request(key, language)).await?;
        let schedule = response
            .json::<ScheduleResponse>()?
            .into_schedule(key.line, key.station, Timestamp::now())
            .inspect_err(|data_error| {
                warn!(
                    error = data_error as &dyn std::error::Error,
                    "Next Train API returned data that could not be converted"
                );
            })?;

        Ok((response, schedule))
    }
}
