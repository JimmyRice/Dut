//! Console logging.

mod request_blocks;

use std::{
    env,
    io::{self, IsTerminal},
};

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use self::request_blocks::RequestBlocks;
use crate::StartupError;

pub(crate) use self::request_blocks::REQUEST_SPAN;

/// Used when `RUST_LOG` is not set: this crate and HTTP request logs at debug
/// level, which includes cache hits, and everything else at info.
const DEFAULT_FILTER: &str = "info,dut=debug,tower_http=debug";

/// Installs a structured console logger for the whole process.
///
/// A person watching a terminal gets each request as one block, from
/// "started processing request" to "end of stream", with a summary line and
/// a blank line around it. Anything else, such as a file or a log collector,
/// gets one uncoloured line per event, as it happens, so it stays greppable.
pub fn init() -> Result<(), StartupError> {
    let installed = if io::stdout().is_terminal() {
        tracing_subscriber::registry()
            .with(filter())
            .with(RequestBlocks::new(io::stdout, colour_wanted()))
            .try_init()
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter())
            .with_ansi(false)
            .finish()
            .try_init()
    };
    installed.map_err(StartupError::Telemetry)
}

fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

/// Honours the `NO_COLOR` convention, as `tracing`'s own formatter does.
fn colour_wanted() -> bool {
    env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}
