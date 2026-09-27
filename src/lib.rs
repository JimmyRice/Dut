mod app;
mod bootstrap;
mod server;

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod state;
pub mod telemetry;

pub use bootstrap::build_app;
pub use config::{AppConfig, MtrConfig};
pub use error::StartupError;
pub use server::run;
