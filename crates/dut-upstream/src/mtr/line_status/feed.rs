use std::time::Duration;

use reqwest::Url;
use thiserror::Error;
use tracing::debug;

use dut_core::{
    application::{feed::Feed, source::SourceUnavailable},
    domain::{
        line_status::{LineCondition, NetworkStatus},
        source_health::SourceId,
        time::HONG_KONG,
    },
};
use dut_http::{OutboundHttpClient, UpstreamError, UpstreamRequest};

use crate::mtr::line_status::dto::{LineStatusDataError, LineStatusFeed};

const UPSTREAM: &str = "mtr.line_status";

/// The condition of every line, read whole from the MTR line status feed.
///
/// The feed is one small document for the whole network. It is read on a
/// schedule rather than per request, so this adapter holds no cache.
#[derive(Clone, Debug)]
pub struct MtrLineStatusFeed {
    http: OutboundHttpClient,
    endpoint: Url,
    request_timeout: Duration,
}

impl MtrLineStatusFeed {
    pub const fn new(http: OutboundHttpClient, endpoint: Url, request_timeout: Duration) -> Self {
        Self {
            http,
            endpoint,
            request_timeout,
        }
    }

    /// The request the startup connectivity check sends: the same request
    /// every poll makes.
    pub fn probe(&self) -> UpstreamRequest {
        self.request()
    }

    fn request(&self) -> UpstreamRequest {
        UpstreamRequest {
            upstream: UPSTREAM,
            url: self.endpoint.clone(),
            timeout: self.request_timeout,
        }
    }

    async fn fetch_status(&self) -> Result<NetworkStatus, FetchError> {
        let response = self.http.fetch(self.request()).await?;
        let status = response.json::<LineStatusFeed>()?.into_network_status()?;

        let affected = status
            .lines
            .iter()
            .filter(|entry| {
                !matches!(
                    entry.condition,
                    LineCondition::Normal | LineCondition::NonServiceHours
                )
            })
            .count();
        debug!(
            lines = status.lines.len(),
            affected_lines = affected,
            updated_at = %status.updated_at.display_with_offset(HONG_KONG),
            "decoded line status feed"
        );

        Ok(status)
    }
}

impl Feed for MtrLineStatusFeed {
    type Item = NetworkStatus;

    const SOURCE: SourceId = SourceId::MtrLineStatus;

    async fn fetch(&self) -> Result<NetworkStatus, SourceUnavailable> {
        self.fetch_status().await.map_err(SourceUnavailable::new)
    }
}

#[derive(Debug, Error)]
enum FetchError {
    #[error(transparent)]
    Upstream(#[from] UpstreamError),

    #[error("the line status feed returned malformed JSON")]
    Json(#[from] serde_json::Error),

    #[error("the line status feed returned unusable data")]
    Data(#[from] LineStatusDataError),
}
