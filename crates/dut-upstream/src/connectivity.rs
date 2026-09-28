//! A one-off check, run at startup, that every upstream can be reached.

use std::error::Error;

use csv::ReaderBuilder;
use futures::future::join_all;
use serde::de::IgnoredAny;
use thiserror::Error;
use tokio::time::Instant;
use tracing::{info, warn};

use dut_http::{OutboundHttpClient, UpstreamError, UpstreamRequest};
use dut_telemetry::millis;

/// Requests one document from every upstream and logs whether each answered.
///
/// It runs once when the server starts, so a host without a route to the
/// internet, a DNS failure, or a blocked upstream shows up in the log before
/// the first rider asks for data. It never stops the server: the caches
/// already retry and fall back to stale data, so an upstream that is down at
/// startup is reported and left to recover.
#[derive(Clone, Debug)]
pub struct ConnectivityCheck {
    http: OutboundHttpClient,
    probes: Vec<Probe>,
}

/// A request the check sends, and the kind of document a real answer is.
///
/// Proxies and captive portals answer `200` with an HTML page, so a
/// successful status alone does not show that upstream was reached; the
/// body must also be the kind of document upstream serves.
#[derive(Clone, Debug)]
pub struct Probe {
    request: UpstreamRequest,
    format: Format,
}

#[derive(Clone, Copy, Debug)]
enum Format {
    Json,
    Csv,
}

impl Probe {
    /// Expects a well-formed JSON document.
    pub const fn json(request: UpstreamRequest) -> Self {
        Self {
            request,
            format: Format::Json,
        }
    }

    /// Expects a CSV file: a header of two or more columns, and rows with
    /// as many fields.
    pub const fn csv(request: UpstreamRequest) -> Self {
        Self {
            request,
            format: Format::Csv,
        }
    }
}

/// How one upstream answered its probe.
#[derive(Debug)]
pub struct ProbeOutcome {
    pub upstream: &'static str,
    pub result: Result<(), ProbeError>,
}

/// Why an upstream counts as unreachable.
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error(transparent)]
    Upstream(#[from] UpstreamError),

    #[error("the response was not JSON")]
    NotJson(#[from] serde_json::Error),

    #[error("the response was not CSV")]
    NotCsv(#[source] Option<csv::Error>),
}

impl ConnectivityCheck {
    pub const fn new(http: OutboundHttpClient, probes: Vec<Probe>) -> Self {
        Self { http, probes }
    }

    /// Probes every upstream at once, logging each outcome as it arrives and
    /// then a summary. Outcomes are returned in probe order.
    pub async fn run(self) -> Vec<ProbeOutcome> {
        info!(
            upstreams = self.probes.len(),
            "checking that upstreams are reachable"
        );
        let outcomes = join_all(self.probes.iter().map(|request| self.probe(request))).await;

        let unreachable: Vec<_> = outcomes
            .iter()
            .filter(|outcome| outcome.result.is_err())
            .map(|outcome| outcome.upstream)
            .collect();
        let reachable = outcomes.len() - unreachable.len();
        if unreachable.is_empty() {
            info!(reachable, "every upstream is reachable");
        } else {
            warn!(
                reachable,
                unreachable = ?unreachable,
                "some upstreams are unreachable"
            );
        }
        outcomes
    }

    async fn probe(&self, probe: &Probe) -> ProbeOutcome {
        let upstream = probe.request.upstream;
        let host = probe.request.url.host_str().unwrap_or_default();
        let started = Instant::now();
        let result = self.fetch(probe).await;
        let elapsed_ms = millis(started.elapsed());

        match &result {
            Ok(()) => info!(upstream, host, elapsed_ms, "upstream reachable"),
            Err(probe_error) => warn!(
                upstream,
                host,
                elapsed_ms,
                error = probe_error as &dyn Error,
                "upstream unreachable"
            ),
        }
        ProbeOutcome { upstream, result }
    }

    /// Fetches the probe and checks that the body is the document expected,
    /// without building a value from it.
    async fn fetch(&self, probe: &Probe) -> Result<(), ProbeError> {
        let response = self.http.fetch(probe.request.clone()).await?;
        match probe.format {
            Format::Json => {
                response.json::<IgnoredAny>()?;
            }
            Format::Csv => check_csv(response.body())?,
        }
        Ok(())
    }
}

/// An HTML page read as CSV has one column, or rows of uneven length.
fn check_csv(body: &[u8]) -> Result<(), ProbeError> {
    let mut reader = ReaderBuilder::new().from_reader(body);
    let columns = reader
        .headers()
        .map_err(|cause| ProbeError::NotCsv(Some(cause)))?
        .len();
    if columns < 2 {
        return Err(ProbeError::NotCsv(None));
    }
    for record in reader.records() {
        record.map_err(|cause| ProbeError::NotCsv(Some(cause)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dut_http::ProxyMode;
    use reqwest::{StatusCode, Url};
    use tokio::net::TcpListener;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::*;
    use crate::fixtures;

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn request(upstream: &'static str, url: &str) -> UpstreamRequest {
        UpstreamRequest {
            upstream,
            url: Url::parse(url).expect("test URL should be valid"),
            timeout: TIMEOUT,
        }
    }

    fn probe(upstream: &'static str, url: &str) -> Probe {
        Probe::json(request(upstream, url))
    }

    async fn run(probes: Vec<Probe>) -> Vec<ProbeOutcome> {
        let http =
            dut_http::build("dut-test", TIMEOUT, ProxyMode::Direct).expect("client should build");
        ConnectivityCheck::new(http, probes).run().await
    }

    async fn serve(route: &str, response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn reports_each_upstream_in_probe_order() {
        let healthy = serve(
            "/feed.json",
            ResponseTemplate::new(200).set_body_bytes(fixtures::read("mtr/line_status.json")),
        )
        .await;
        let failing = serve("/feed.json", ResponseTemplate::new(503)).await;

        let outcomes = run(vec![
            probe("healthy", &format!("{}/feed.json", healthy.uri())),
            probe("failing", &format!("{}/feed.json", failing.uri())),
        ])
        .await;

        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].upstream, "healthy");
        assert!(outcomes[0].result.is_ok());
        assert_eq!(outcomes[1].upstream, "failing");
        assert!(matches!(
            outcomes[1].result,
            Err(ProbeError::Upstream(UpstreamError::Status {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn treats_a_successful_response_that_is_not_json_as_unreachable() {
        let portal = serve(
            "/feed.json",
            ResponseTemplate::new(200).set_body_string("<html>Sign in to continue</html>"),
        )
        .await;

        let outcomes = run(vec![probe(
            "portal",
            &format!("{}/feed.json", portal.uri()),
        )])
        .await;

        assert!(matches!(outcomes[0].result, Err(ProbeError::NotJson(_))));
    }

    #[tokio::test]
    async fn accepts_a_csv_file_where_csv_is_expected() {
        let portal = serve(
            "/data/airport_express_fares.csv",
            ResponseTemplate::new(200)
                .set_body_bytes(fixtures::read("mtr/open_data/airport_express_fares.csv")),
        )
        .await;

        let outcomes = run(vec![Probe::csv(request(
            "open_data",
            &format!("{}/data/airport_express_fares.csv", portal.uri()),
        ))])
        .await;

        assert!(outcomes[0].result.is_ok());
    }

    #[tokio::test]
    async fn treats_an_html_page_as_not_csv() {
        let portal = serve(
            "/data/airport_express_fares.csv",
            ResponseTemplate::new(200)
                .set_body_string("<!DOCTYPE html>\n<html>Sign in to continue</html>\n"),
        )
        .await;

        let outcomes = run(vec![Probe::csv(request(
            "open_data",
            &format!("{}/data/airport_express_fares.csv", portal.uri()),
        ))])
        .await;

        assert!(matches!(outcomes[0].result, Err(ProbeError::NotCsv(_))));
    }

    #[tokio::test]
    async fn treats_a_refused_connection_as_unreachable() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("should bind a free port");
        let address = listener.local_addr().expect("should read the port");
        drop(listener);

        let outcomes = run(vec![probe("closed", &format!("http://{address}/"))]).await;

        assert!(
            matches!(
                outcomes[0].result,
                Err(ProbeError::Upstream(UpstreamError::Transport { .. }))
            ),
            "{:?}",
            outcomes[0].result
        );
    }
}
