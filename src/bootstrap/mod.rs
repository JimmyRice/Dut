//! The composition root and process lifecycle: configuration, construction
//! of every concrete dependency, and serving the result. Nothing else in the
//! crate builds infrastructure.

mod config;
mod error;
mod server;

pub use config::AppConfig;
pub use error::StartupError;
pub use server::run;

use axum::Router;
use reqwest::Url;

use crate::{
    api,
    application::{line_status::LineStatusService, next_train::NextTrainService},
    infrastructure::{
        connectivity::ConnectivityCheck,
        http_client::{self, UpstreamRequest},
        mtr::{line_status::MtrLineStatusSource, next_train::MtrNextTrainSource},
    },
    state::AppState,
};

/// Builds the complete HTTP application, constructing every concrete
/// dependency once. Tests call it directly to get a router without binding a
/// socket.
pub fn build_app(config: &AppConfig) -> Result<Router, StartupError> {
    assemble(config).map(|app| app.router)
}

/// Everything [`run`] starts: the router, and the check that upstreams can
/// be reached, which shares the router's HTTP client.
struct App {
    router: Router,
    connectivity: ConnectivityCheck,
}

fn assemble(config: &AppConfig) -> Result<App, StartupError> {
    let outbound_http =
        http_client::build(config.outbound_http_timeout()).map_err(StartupError::HttpClient)?;
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
        router: api::router(state),
        connectivity,
    })
}

fn parse_endpoint(name: &'static str, value: &str) -> Result<Url, StartupError> {
    Url::parse(value).map_err(|source| StartupError::InvalidEndpoint { name, source })
}
