use axum::Router;
use reqwest::Url;

use crate::{
    AppConfig, StartupError, app,
    application::{line_status::LineStatusService, next_train::NextTrainService},
    infrastructure::{
        http_client,
        mtr::{line_status::MtrLineStatusSource, next_train::MtrNextTrainSource},
    },
    state::AppState,
};

pub fn build_app(config: &AppConfig) -> Result<Router, StartupError> {
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
        outbound_http,
        parse_endpoint("line_status_endpoint", &mtr.line_status_endpoint)?,
        mtr.request_timeout,
        mtr.line_status_cache,
    );

    let state = AppState::new(
        NextTrainService::new(next_trains),
        LineStatusService::new(line_status),
    );

    Ok(app::build_router(state))
}

fn parse_endpoint(name: &'static str, value: &str) -> Result<Url, StartupError> {
    Url::parse(value).map_err(|source| StartupError::InvalidEndpoint { name, source })
}
