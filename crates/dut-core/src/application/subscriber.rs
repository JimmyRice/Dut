//! Port for business logic that reacts to monitor events.

use std::future::Future;

use crate::domain::event::MonitorEvent;

/// Reacts to the events the monitor publishes, such as sending push
/// notifications when a line is disrupted.
///
/// Each subscriber runs in its own task and receives every event in the
/// order it was published. It owns its state through `&mut self`, so it
/// needs no locks, and a slow subscriber delays only itself. It lives here
/// rather than beside the monitor so that implementations depend on
/// `dut-core` alone.
pub trait Subscriber: Send + 'static {
    /// Short, stable name used in logs, such as `push`.
    const NAME: &'static str;

    /// Handles one event. The next event is delivered once this finishes.
    fn on_event(&mut self, event: &MonitorEvent) -> impl Future<Output = ()> + Send;

    /// Called when the subscriber fell so far behind that `missed` events
    /// were dropped before it could receive them. Delivery then continues
    /// with the oldest event still available.
    ///
    /// Does nothing by default. Override it to resynchronise from the feeds'
    /// latest values when missing a change would matter.
    fn on_lagged(&mut self, missed: u64) -> impl Future<Output = ()> + Send {
        let _ = missed;
        async {}
    }
}
