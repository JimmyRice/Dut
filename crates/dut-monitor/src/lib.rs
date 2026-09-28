//! Watches the polled feeds and publishes every change as a
//! [`MonitorEvent`](dut_core::domain::event::MonitorEvent).
//!
//! The monitor reports facts, not judgements: routine changes such as
//! service ending for the night are published like disruptions, and nothing
//! is ranked by severity. Subscribers such as push notifications decide what
//! matters to their audience.
//!
//! Each feed's first successful poll is a baseline and publishes nothing, so
//! a restart never replays the current state as news. A failed poll changes
//! no data, so an outage is reported only as a source health change and
//! never as service resuming.
//!
//! Business logic reacts to events by implementing
//! [`Subscriber`](dut_core::application::subscriber::Subscriber) and being
//! attached through [`MonitorHandle::attach`]. The event log is itself such
//! a subscriber, attached by [`spawn`].
//!
//! Only one instance should run the monitor, since each would publish the
//! same events.

mod delivery;
mod event_log;
mod monitor;
mod signals;
mod watcher;

pub use monitor::{MonitorHandle, spawn};
pub use signals::NextTrainSignalFeed;
