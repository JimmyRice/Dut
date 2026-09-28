//! The composition root and process lifecycle: configuration, construction
//! of every concrete dependency, and serving the result. Nothing else in the
//! workspace builds infrastructure.

mod app;
mod config;
mod error;
mod server;

pub use app::build_app;
pub use config::AppConfig;
pub use error::StartupError;
pub use server::run;
