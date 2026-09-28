//! Builds every concrete dependency once and wires them into the
//! application.

use axum::Router;
use reqwest::Url;

use dut_api::AppState;
use dut_core::application::{line_status::LineStatusService, next_train::NextTrainService};
use dut_http::UpstreamRequest;
use dut_upstream::{
    connectivity::ConnectivityCheck,
    mtr::{line_status::MtrLineStatusSource, next_train::MtrNextTrainSource},
};

use super::{AppConfig, StartupError};

/// Identifies this service to upstreams, e.g. `dut/0.1.0`.
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Builds the complete HTTP application, constructing every concrete
/// dependency once. Tests call it directly to get a router without binding a
/// socket.
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

    let next_trains = MtrNextTrainSource::new(
        outbound_http.clone(),
        parse_endpoint("next_train_endpoint", &mtr.next_train_endpoint)?,
        mtr.request_timeout,
        mtr.next_train_cache,
    );
    let line_status = MtrLineStatusSource::new(
        outbound_http.clone(),
        parse_endpoint("line_status_endpoint", &mtr.line_status_endpoint)?,
        mtr.request_timeout,
        mtr.line_status_cache,
    );
    let weather = UpstreamRequest {
        upstream: "hko.weather",
        url: parse_endpoint("weather_endpoint", config.weather_endpoint())?,
        timeout: config.outbound_http_timeout(),
    };
    let connectivity = ConnectivityCheck::new(
        outbound_http,
        vec![next_trains.probe(), line_status.probe(), weather],
    );

    let state = AppState::new(
        NextTrainService::new(next_trains),
        LineStatusService::new(line_status),
    );

    Ok(App {
        router: dut_api::router(state),
        connectivity,
    })
}

fn parse_endpoint(name: &'static str, value: &str) -> Result<Url, StartupError> {
    Url::parse(value).map_err(|source| StartupError::InvalidEndpoint { name, source })
}
