//! Route-level tests: the application wired to a fake MTR upstream, one
//! module per endpoint group. They share one test binary, so adding an
//! endpoint does not add another crate to compile and link.

mod line_status;
mod lines;
mod next_train;
mod not_found;
mod support;
