//! The composition root and process lifecycle: the command line,
//! configuration, construction of every concrete dependency, and serving the
//! result. Nothing else in the workspace builds infrastructure.

mod app;
mod command_line;
mod config;
mod error;
mod server;

pub use app::build_app;
pub use command_line::CommandLine;
pub use config::AppConfig;
pub use error::StartupError;
pub use server::run;
