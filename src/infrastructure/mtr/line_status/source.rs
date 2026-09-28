use std::{fmt, time::Duration};

use reqwest::Url;
use thiserror::Error;
use tracing::debug;

use crate::{
    application::{
        line_status::LineStatusSource,
        source::{Snapshot, SourceUnavailable},
    },
    domain::{
        line_status::{LineCondition, NetworkStatus},
        time::HONG_KONG,
    },
    infrastructure::{
        cache::{CachePolicy, Fetched, RefreshingCache},
        http_client::{OutboundHttpClient, UpstreamError, UpstreamRequest},
        mtr::line_status::dto::{LineStatusDataError, LineStatusFeed},
    },
};

const UPSTREAM: &str = "mtr.line_status";

/// The condition of every line from the MTR line status feed.
///
/// The whole network is one feed, so the cache holds a single entry.
#[derive(Debug)]
pub struct MtrLineStatusSource {
    client: LineStatusClient,
    cache: RefreshingCache<AllLines, NetworkStatus>,
}

impl MtrLineStatusSource {
    pub fn new(
        http: OutboundHttpClient,
        endpoint: Url,
        request_timeout: Duration,
        policy: CachePolicy,
    ) -> Self {
        Self {
            client: LineStatusClient {
                http,
                endpoint,
                request_timeout,
            },
            cache: RefreshingCache::new(UPSTREAM, policy),
        }
    }

    /// The request the startup connectivity check sends: the whole feed,
    /// which is one small document.
    pub fn probe(&self) -> UpstreamRequest {
        self.client.request()
    }
}

impl LineStatusSource for MtrLineStatusSource {
    async fn status(&self) -> Result<Snapshot<NetworkStatus>, SourceUnavailable> {
        let client = self.client.clone();
        self.cache
            .get(&AllLines, move || async move { client.fetch().await })
            .await
            .map_err(SourceUnavailable::new)
    }
}

/// The single cache key of the line status feed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct AllLines;

impl fmt::Display for AllLines {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("all_lines")
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

#[derive(Clone, Debug)]
struct LineStatusClient {
    http: OutboundHttpClient,
    endpoint: Url,
    request_timeout: Duration,
}

impl LineStatusClient {
    fn request(&self) -> UpstreamRequest {
        UpstreamRequest {
            upstream: UPSTREAM,
            url: self.endpoint.clone(),
            timeout: self.request_timeout,
        }
    }

    async fn fetch(&self) -> Result<Fetched<NetworkStatus>, FetchError> {
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

        Ok(Fetched {
            value: status,
            ttl_hint: response.ttl_hint(),
        })
    }
}
