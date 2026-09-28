//! Background polling of whole-document feeds, such as the MTR line status.
//!
//! [`spawn`] reads a [`Feed`](dut_core::application::feed::Feed) on its own
//! [`Schedule`] in its own task, so a slow upstream never delays another. The
//! last value read and the source's health stay available through a
//! [`FeedHandle`]: the line status endpoint serves from it, and the monitor
//! watches it for changes.
//!
//! This crate knows nothing of what the feeds contain. It runs in every API
//! instance, whereas the monitor, which interprets changes, runs in one.

mod handle;
mod health;
mod line_status;
mod poller;
mod reference_data;
mod schedule;
mod state;

pub use handle::FeedHandle;
pub use poller::spawn;
pub use schedule::Schedule;
pub use state::{FeedState, Polled};
