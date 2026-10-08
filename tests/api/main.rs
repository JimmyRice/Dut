//! Route-level tests: the application wired to fake MTR and Observatory
//! upstreams, one module per endpoint group. They share one test binary, so adding an
//! endpoint does not add another crate to compile and link.

// `clippy.toml` exempts `#[test]` functions only, not the helpers they share.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod data;
mod health;
mod line_status;
mod lines;
mod mock;
mod next_train;
mod not_found;
mod support;
