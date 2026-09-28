//! Adapters for the upstream services this API reads, and the cache that
//! shields those services from request volume.
//!
//! Each adapter implements a port from `dut-core`, so the use cases never see
//! where their data comes from or how it is cached.

mod cache;
pub mod connectivity;
pub mod mtr;

#[cfg(test)]
mod fixtures;

pub use cache::CachePolicy;
