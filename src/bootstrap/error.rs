use std::{io, net::SocketAddr};

use thiserror::Error;

/// Why the process could not start, or stopped serving. Only `main` sees it,
/// so unlike API errors it may name addresses, files, and upstream URLs.
///
/// Invalid command line options never get this far: [`CommandLine::read`]
/// reports them and exits first.
///
/// [`CommandLine::read`]: super::CommandLine::read
#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to start logging: {0}")]
    Telemetry(#[from] dut_telemetry::InitError),

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
