mod bootstrap;

pub mod api;
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod state;
pub mod telemetry;

pub use bootstrap::{AppConfig, MtrConfig, StartupError, build_app, run};
