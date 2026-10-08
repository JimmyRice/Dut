//! Builds every concrete dependency once and wires them into the
//! application.

use axum::Router;
use reqwest::Url;

use dut_api::AppState;
use dut_core::application::{
    line_status::LineStatusService, next_train::NextTrainService,
    reference_data::ReferenceDataService,
};
use dut_monitor::NextTrainSignalFeed;
use dut_upstream::{
    connectivity::ConnectivityCheck,
    hko::warnings::HkoWarningFeed,
    mtr::{
        line_status::MtrLineStatusFeed, next_train::MtrNextTrainSource, open_data::MtrOpenDataFeed,
    },
};

use super::{AppConfig, StartupError};

/// Identifies this service to upstreams, e.g. `dut/0.1.0`.
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Builds the complete HTTP application, constructing every concrete
/// dependency once. Tests call it directly to get a router without binding a
/// socket.
///
/// It also starts polling the background feeds and the monitor that watches
/// them, so it must be called within a Tokio runtime, and a fake upstream
/// must be ready before it is called.
pub fn build_app(config: &AppConfig) -> Result<Router, StartupError> {
    assemble(config).map(|app| app.router)
}

/// Everything [`run`](super::run) starts: the router, and the check that
/// upstreams can be reached, which shares the router's HTTP client.
pub(super) struct App {
    pub(super) router: Router,
    pub(super) connectivity: ConnectivityCheck,
}

pub(super) fn assemble(config: &AppConfig) -> Result<App, StartupError> {
    let outbound_http = dut_http::build(
        USER_AGENT,
        config.outbound_http_timeout(),
        config.outbound_proxy(),
    )
    .map_err(StartupError::HttpClient)?;
    let mtr = config.mtr();
    let hko = config.hko();
    let polling = config.polling();

    let next_train_source = MtrNextTrainSource::new(
        outbound_http.clone(),
        parse_endpoint("next_train_endpoint", &mtr.next_train_endpoint)?,
        mtr.request_timeout,
        mtr.next_train_cache,
    );
    let line_status_feed = MtrLineStatusFeed::new(
        outbound_http.clone(),
        parse_endpoint("line_status_endpoint", &mtr.line_status_endpoint)?,
        mtr.request_timeout,
    );
    let open_data_feed = MtrOpenDataFeed::new(
        outbound_http.clone(),
        &parse_endpoint("open_data_endpoint", &mtr.open_data_endpoint)?,
        mtr.open_data_timeout,
    )
    .map_err(|source| StartupError::InvalidEndpoint {
        name: "open_data_endpoint",
        source,
    })?;
    let weather_warnings_feed = HkoWarningFeed::new(
        outbound_http.clone(),
        parse_endpoint("weather_warnings_endpoint", &hko.warnings_endpoint)?,
        hko.request_timeout,
    );
    let connectivity = ConnectivityCheck::new(
        outbound_http,
        vec![
            next_train_source.probe(),
            line_status_feed.probe(),
            weather_warnings_feed.probe(),
            open_data_feed.probe(),
        ],
    );

    let next_trains = NextTrainService::new(next_train_source);
    let line_status = dut_poll::spawn(line_status_feed, polling.line_status);
    let weather_warnings = dut_poll::spawn(weather_warnings_feed, polling.weather_warnings);
    let next_train_signals = dut_poll::spawn(
        NextTrainSignalFeed::new(next_trains.clone()),
        polling.next_train_signals,
    );
    // The monitor attaches its own event log. Business logic that reacts to
    // events is attached here too, with `monitor.attach(subscriber)`.
    let _monitor = dut_monitor::spawn(&line_status, &weather_warnings, &next_train_signals);

    let reference_data = dut_poll::spawn(open_data_feed, polling.open_data);

    let state = AppState::new(
        next_trains,
        LineStatusService::new(line_status),
        ReferenceDataService::new(reference_data),
    )
    .with_mock_api(config.mock_api());

    Ok(App {
        router: dut_api::router(state),
        connectivity,
    })
}

fn parse_endpoint(name: &'static str, value: &str) -> Result<Url, StartupError> {
    Url::parse(value).map_err(|source| StartupError::InvalidEndpoint { name, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults are only parsed when the real service starts, which no
    /// other test does, since that would poll the live upstreams.
    #[test]
    fn the_default_endpoints_are_valid_urls() {
        let config = AppConfig::default();
        let (mtr, hko) = (config.mtr(), config.hko());

        for (name, endpoint) in [
            ("next_train_endpoint", &mtr.next_train_endpoint),
            ("line_status_endpoint", &mtr.line_status_endpoint),
            ("open_data_endpoint", &mtr.open_data_endpoint),
            ("weather_warnings_endpoint", &hko.warnings_endpoint),
        ] {
            assert!(parse_endpoint(name, endpoint).is_ok(), "{name}: {endpoint}");
        }
    }
}
