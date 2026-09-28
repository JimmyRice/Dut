//! The HTTP surface: Axum routes, request and response DTOs, error mapping,
//! and request tracing.
//!
//! Handlers only extract input, call a service from `dut-core`, and map the
//! result to a response. The services' data sources are type parameters, so
//! this crate never depends on an adapter.

mod dto;
mod error;
mod http_cache;
mod middleware;
mod router;
mod routes;
mod state;

pub use router::router;
pub use state::AppState;
