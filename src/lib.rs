//! The `dut` binary's composition root. Every other part of the service
//! lives in a crate under `crates/`; this one reads the command line,
//! builds each concrete dependency once, and serves the result.

mod bootstrap;

pub use bootstrap::{AppConfig, CommandLine, StartupError, build_app, run};
