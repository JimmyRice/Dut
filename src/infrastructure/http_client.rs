//! The single gateway for outbound HTTP.
//!
//! Every upstream call goes through [`OutboundHttpClient::fetch`], so each
//! one is logged the same way: when it starts, how long it took, what came
//! back, and how upstream wants it cached.

use std::{error::Error, time::Duration};

use bytes::Bytes;
use reqwest::{
    StatusCode, Url,
    header::{AGE, CACHE_CONTROL, HeaderMap},
};
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::time::Instant;
use tracing::{Instrument, error, info, info_span, warn};

use crate::{infrastructure::http_freshness, telemetry::millis};

/// How many bytes of an undecodable body to include in logs.
const BODY_EXCERPT_BYTES: usize = 200;

/// A shared HTTP client. Cloning is cheap and reuses the connection pool.
#[derive(Clone, Debug)]
pub struct OutboundHttpClient {
    inner: reqwest::Client,
}

/// A `GET` request to an upstream service.
#[derive(Clone, Debug)]
pub struct UpstreamRequest {
    /// Short, stable name of the upstream, used in logs, e.g. `mtr.next_train`.
    pub upstream: &'static str,
    pub url: Url,
    /// Overrides the client-wide timeout for this request.
    pub timeout: Duration,
}

/// A successful (2xx) upstream response with its body fully read.
#[derive(Debug)]
pub struct UpstreamResponse {
    upstream: &'static str,
    headers: HeaderMap,
    body: Bytes,
}

#[derive(Debug, Error)]
pub enum UpstreamError {
    #[error("the request to {upstream} failed")]
    Transport {
        upstream: &'static str,
        #[source]
        cause: reqwest::Error,
    },

    #[error("{upstream} responded with HTTP {status}")]
    Status {
        upstream: &'static str,
        status: StatusCode,
    },
}

impl OutboundHttpClient {
    pub const fn new(inner: reqwest::Client) -> Self {
        Self { inner }
    }

    /// Sends `request` and reads the whole response body.
    ///
    /// Non-2xx responses are errors. Every outcome is logged inside an
    /// `upstream` span.
    pub async fn fetch(&self, request: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let UpstreamRequest {
            upstream,
            url,
            timeout,
        } = request;
        let span = info_span!(
            "upstream",
            upstream,
            host = url.host_str().unwrap_or_default(),
            path = url.path(),
        );

        async move {
            info!(
                query = url.query().unwrap_or_default(),
                timeout_ms = millis(timeout),
                "upstream request started"
            );
            let started = Instant::now();
            let transport_error = |cause: reqwest::Error, stage: &'static str| {
                warn!(
                    stage,
                    kind = failure_kind(&cause),
                    elapsed_ms = millis(started.elapsed()),
                    error = &cause as &dyn Error,
                    "upstream request failed"
                );
                UpstreamError::Transport { upstream, cause }
            };

            let response = self
                .inner
                .get(url)
                .timeout(timeout)
                .send()
                .await
                .map_err(|cause| transport_error(cause, "send"))?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response
                .bytes()
                .await
                .map_err(|cause| transport_error(cause, "read_body"))?;

            let elapsed_ms = millis(started.elapsed());
            let cache_control = header_text(&headers, CACHE_CONTROL.as_str());
            let age = header_text(&headers, AGE.as_str());

            if !status.is_success() {
                warn!(
                    status = status.as_u16(),
                    elapsed_ms,
                    bytes = body.len(),
                    "upstream responded with an error status"
                );
                return Err(UpstreamError::Status { upstream, status });
            }

            info!(
                status = status.as_u16(),
                elapsed_ms,
                bytes = body.len(),
                cache_control,
                age,
                "upstream request finished"
            );
            Ok(UpstreamResponse {
                upstream,
                headers,
                body,
            })
        }
        .instrument(span)
        .await
    }
}

impl UpstreamResponse {
    /// Decodes the body as JSON, logging an excerpt of it when that fails.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body).inspect_err(|decode_error| {
            let excerpt = self.body.get(..BODY_EXCERPT_BYTES).unwrap_or(&self.body);
            error!(
                upstream = self.upstream,
                line = decode_error.line(),
                column = decode_error.column(),
                error = decode_error as &dyn Error,
                body_excerpt = %String::from_utf8_lossy(excerpt),
                "upstream returned JSON that could not be decoded"
            );
        })
    }

    /// How much longer upstream considers this response fresh.
    pub fn ttl_hint(&self) -> Option<Duration> {
        http_freshness::ttl_hint(&self.headers)
    }
}

pub(crate) fn build(timeout: Duration) -> Result<OutboundHttpClient, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map(OutboundHttpClient::new)
}

fn failure_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_body() || error.is_decode() {
        "body"
    } else if error.is_request() {
        "request"
    } else {
        "other"
    }
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}
