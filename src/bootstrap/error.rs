use std::{io, net::SocketAddr};

use thiserror::Error;

/// Why the process could not start, or stopped serving. Only `main` sees it,
/// so unlike API errors it may name addresses, files, and upstream URLs.
///
/// Like every error in the workspace, a message leaves out its cause, which
/// `source` returns instead, so that [`report`] prints each cause once.
///
/// Invalid command line options never get this far: [`CommandLine::read`]
/// reports them and exits first.
///
/// [`report`]: fn@super::report
/// [`CommandLine::read`]: super::CommandLine::read
#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to start logging")]
    Telemetry(#[from] dut_telemetry::InitError),

    #[error("failed to build the outbound HTTP client")]
    HttpClient(#[source] reqwest::Error),

    #[error("the configured {name} is not a valid URL")]
    InvalidEndpoint {
        name: &'static str,
        #[source]
        source: url::ParseError,
    },

    #[error("failed to bind the HTTP server to {address}")]
    Bind {
        address: SocketAddr,
        #[source]
        source: io::Error,
    },

    #[error("failed to read the HTTP server's local address")]
    LocalAddress(#[source] io::Error),

    #[error("the HTTP server stopped with an error")]
    Serve(#[source] io::Error),
}
