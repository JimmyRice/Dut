//! Route-level tests: the application wired to fake MTR and Observatory
//! upstreams, one module per endpoint group. They share one test binary, so adding an
//! endpoint does not add another crate to compile and link.

mod data;
mod health;
mod line_status;
mod lines;
mod mock;
mod next_train;
mod not_found;
mod support;
