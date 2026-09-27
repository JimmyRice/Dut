use std::{io, net::SocketAddr};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to install the log subscriber: {0}")]
    Telemetry(#[source] tracing_subscriber::util::TryInitError),

    #[error("failed to build the outbound HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),

    #[error("the configured {name} is not a valid URL: {source}")]
    InvalidEndpoint {
        name: &'static str,
        #[source]
        source: url::ParseError,
    },

    #[error("failed to bind the HTTP server to {address}: {source}")]
    Bind {
        address: SocketAddr,
        #[source]
        source: io::Error,
    },

    #[error("failed to read the HTTP server's local address: {0}")]
    LocalAddress(#[source] io::Error),

    #[error("the HTTP server stopped with an error: {0}")]
    Serve(#[source] io::Error),
}
