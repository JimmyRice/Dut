//! Console logging.

use tracing_subscriber::{EnvFilter, util::SubscriberInitExt};

use crate::StartupError;

/// Used when `RUST_LOG` is not set: this crate and HTTP request logs at debug
/// level, which includes cache hits, and everything else at info.
const DEFAULT_FILTER: &str = "info,dut=debug,tower_http=debug";

/// Installs a structured console logger for the whole process.
pub fn init() -> Result<(), StartupError> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .finish()
        .try_init()
        .map_err(StartupError::Telemetry)
}
