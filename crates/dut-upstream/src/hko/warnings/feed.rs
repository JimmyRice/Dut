use std::time::Duration;

use reqwest::Url;
use thiserror::Error;
use tracing::debug;

use dut_core::{
    application::{feed::Feed, source::SourceUnavailable},
    domain::{source_health::SourceId, weather::WeatherWarnings},
};
use dut_http::{OutboundHttpClient, UpstreamError, UpstreamRequest};

use crate::hko::warnings::dto::WarningInfoResponse;

const UPSTREAM: &str = "hko.warnings";

/// Every weather warning in force, read whole from the Observatory's open
/// data API.
///
/// Warnings change rarely and the document is small, so it is read on a
/// schedule and this adapter holds no cache. English is enough: only the
/// warning codes are read, not their text.
#[derive(Clone, Debug)]
pub struct HkoWarningFeed {
    http: OutboundHttpClient,
    endpoint: Url,
    request_timeout: Duration,
}

impl HkoWarningFeed {
    /// `endpoint` is the full `warningInfo` URL, query included.
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

    async fn fetch_warnings(&self) -> Result<WeatherWarnings, FetchError> {
        let response = self.http.fetch(self.request()).await?;
        let warnings = response.json::<WarningInfoResponse>()?.into_warnings();
        debug!(
            warnings = warnings.warnings.len(),
            "decoded weather warnings"
        );
        Ok(warnings)
    }
}

impl Feed for HkoWarningFeed {
    type Item = WeatherWarnings;

    const SOURCE: SourceId = SourceId::HkoWarnings;

    async fn fetch(&self) -> Result<WeatherWarnings, SourceUnavailable> {
        self.fetch_warnings().await.map_err(SourceUnavailable::new)
    }
}

#[derive(Debug, Error)]
enum FetchError {
    #[error(transparent)]
    Upstream(#[from] UpstreamError),

    #[error("the weather warning document is malformed JSON")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use dut_core::domain::weather::WeatherWarning;
    use dut_http::ProxyMode;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    use super::*;
    use crate::fixtures;

    const PATH: &str = "/weatherAPI/opendata/weather.php";

    async fn feed(upstream: &MockServer) -> HkoWarningFeed {
        let http = dut_http::build("dut-test", Duration::from_secs(5), ProxyMode::Direct)
            .expect("client should build");
        let endpoint = format!("{}{PATH}?dataType=warningInfo&lang=en", upstream.uri());
        HkoWarningFeed::new(
            http,
            endpoint.parse().expect("endpoint should be a URL"),
            Duration::from_secs(1),
        )
    }

    #[tokio::test]
    async fn reads_the_warnings_in_force() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(PATH))
            .and(query_param("dataType", "warningInfo"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(fixtures::read("hko/warning_info.json"), "application/json"),
            )
            .expect(1)
            .mount(&upstream)
            .await;

        let warnings = feed(&upstream)
            .await
            .fetch()
            .await
            .expect("feed should load");

        assert_eq!(warnings.warnings.len(), 1);
        assert_eq!(warnings.warnings[0].warning, WeatherWarning::HotWeather);
    }

    #[tokio::test]
    async fn an_error_status_makes_the_feed_unavailable() {
        let upstream = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(PATH))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&upstream)
            .await;

        let result = feed(&upstream).await.fetch().await;

        assert!(result.is_err());
    }
}
