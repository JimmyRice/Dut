use std::{sync::Arc, time::Duration};

use reqwest::Url;
use tracing::info;

use dut_core::{
    application::{feed::Feed, source::SourceUnavailable},
    domain::{
        reference::{BySourceFile, PublishedFile, ReferenceData, SourceFile},
        source_health::SourceId,
    },
};
use dut_http::{OutboundHttpClient, UpstreamRequest};

use crate::{
    connectivity::Probe,
    mtr::open_data::{clean, error::OpenDataError},
};

const UPSTREAM: &str = "mtr.open_data";

/// Every file this service mirrors from the MTR's open data portal, read
/// whole and cleaned into datasets.
///
/// The files change a few times a year, so they are read on a slow
/// schedule and held by the poller; this adapter holds no cache. All seven
/// are read together and a poll succeeds only if every one does, so the
/// datasets always come from one consistent set of files.
#[derive(Clone, Debug)]
pub struct MtrOpenDataFeed {
    http: OutboundHttpClient,
    urls: BySourceFile<Url>,
    request_timeout: Duration,
}

impl MtrOpenDataFeed {
    /// `base_url` is the portal's data directory, ending in `/`, such as
    /// `https://opendata.mtr.com.hk/data/`. Fails when a file name cannot
    /// be joined onto it.
    pub fn new(
        http: OutboundHttpClient,
        base_url: &Url,
        request_timeout: Duration,
    ) -> Result<Self, url::ParseError> {
        let urls = BySourceFile::try_from_fn(|file| base_url.join(file.file_name()))?;
        Ok(Self {
            http,
            urls,
            request_timeout,
        })
    }

    /// The request the startup connectivity check sends: the smallest file.
    pub fn probe(&self) -> Probe {
        Probe::csv(self.request(SourceFile::AirportExpressFares))
    }

    fn request(&self, file: SourceFile) -> UpstreamRequest {
        UpstreamRequest {
            upstream: UPSTREAM,
            url: self.urls.get(file).clone(),
            timeout: self.request_timeout,
        }
    }

    async fn read(&self) -> Result<ReferenceData, OpenDataError> {
        let (
            lines_and_stations,
            lines_fares,
            airport_express_fares,
            light_rail_routes_and_stops,
            light_rail_fares,
            barrier_free_facility_categories,
            barrier_free_facilities,
        ) = futures::try_join!(
            self.download(SourceFile::LinesAndStations),
            self.download(SourceFile::LinesFares),
            self.download(SourceFile::AirportExpressFares),
            self.download(SourceFile::LightRailRoutesAndStops),
            self.download(SourceFile::LightRailFares),
            self.download(SourceFile::BarrierFreeFacilityCategories),
            self.download(SourceFile::BarrierFreeFacilities),
        )?;
        let data = clean::reference_data(BySourceFile {
            lines_and_stations,
            lines_fares,
            airport_express_fares,
            light_rail_routes_and_stops,
            light_rail_fares,
            barrier_free_facility_categories,
            barrier_free_facilities,
        })?;

        info!(
            stations = data.stations.value().stations.len(),
            fares = data.fares.value().trips().len(),
            airport_express_fares = data.airport_express_fares.value().trips().len(),
            light_rail_stops = data.light_rail.value().stops.len(),
            light_rail_fares = data.light_rail_fares.value().trips().len(),
            accessible_stations = data.accessibility.value().stations.len(),
            "cleaned open data"
        );
        Ok(data)
    }

    async fn download(&self, file: SourceFile) -> Result<PublishedFile, OpenDataError> {
        let response = self.http.fetch(self.request(file)).await?;
        Ok(PublishedFile::new(
            Arc::from(response.body().as_ref()),
            response.last_modified(),
        ))
    }
}

impl Feed for MtrOpenDataFeed {
    type Item = ReferenceData;

    const SOURCE: SourceId = SourceId::MtrOpenData;

    async fn fetch(&self) -> Result<ReferenceData, SourceUnavailable> {
        self.read().await.map_err(SourceUnavailable::new)
    }
}

#[cfg(test)]
mod tests {
    use dut_http::ProxyMode;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::*;
    use crate::fixtures;

    const TIMEOUT: Duration = Duration::from_secs(5);

    async fn feed(server: &MockServer) -> MtrOpenDataFeed {
        let http =
            dut_http::build("dut-test", TIMEOUT, ProxyMode::Direct).expect("client should build");
        let base = Url::parse(&format!("{}/data/", server.uri())).expect("valid base URL");
        MtrOpenDataFeed::new(http, &base, TIMEOUT).expect("file URLs should join")
    }

    async fn serve_captured(server: &MockServer, except: Option<SourceFile>) {
        for file in SourceFile::ALL {
            let response = if Some(file) == except {
                ResponseTemplate::new(500)
            } else {
                ResponseTemplate::new(200)
                    .insert_header("last-modified", "Thu, 02 Apr 2026 17:02:50 GMT")
                    .set_body_bytes(fixtures::read(&format!(
                        "mtr/open_data/{}",
                        file.file_name()
                    )))
            };
            Mock::given(method("GET"))
                .and(path(format!("/data/{}", file.file_name())))
                .respond_with(response)
                .mount(server)
                .await;
        }
    }

    #[tokio::test]
    async fn reads_every_file_and_dates_them() {
        let server = MockServer::start().await;
        serve_captured(&server, None).await;

        let data = feed(&server)
            .await
            .fetch()
            .await
            .expect("every file is served");

        let updated_at = "2026-04-02T17:02:50Z".parse().ok();
        assert_eq!(data.fares.updated_at(), updated_at);
        assert_eq!(
            data.files.get(SourceFile::LinesFares).updated_at(),
            updated_at
        );
        assert_eq!(data.light_rail.value().stops.len(), 68);
    }

    #[tokio::test]
    async fn fails_the_whole_poll_when_one_file_fails() {
        let server = MockServer::start().await;
        serve_captured(&server, Some(SourceFile::BarrierFreeFacilities)).await;

        assert!(feed(&server).await.fetch().await.is_err());
    }

    #[test]
    fn joins_file_names_onto_the_base_url() {
        let http =
            dut_http::build("dut-test", TIMEOUT, ProxyMode::Direct).expect("client should build");
        let base = Url::parse("https://opendata.mtr.com.hk/data/").expect("valid base URL");
        let feed = MtrOpenDataFeed::new(http, &base, TIMEOUT).expect("file URLs should join");

        assert_eq!(
            feed.request(SourceFile::LinesFares).url.as_str(),
            "https://opendata.mtr.com.hk/data/mtr_lines_fares.csv"
        );
    }
}
