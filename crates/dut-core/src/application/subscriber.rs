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
///
/// # Examples
///
/// Notice when a line ends service for the night. The decision is a pure
/// function, so it can be tested without a task, and `on_event` keeps only
/// the side effect.
///
/// ```
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// use dut_core::{
///     application::subscriber::Subscriber,
///     domain::{
///         event::{Change, MonitorEvent},
///         line_status::{LineCondition, LineStatus, LineStatusChange},
///         network::Line,
///     },
/// };
/// use jiff::Timestamp;
///
/// /// Remembers which lines have ended service tonight.
/// #[derive(Debug, Default)]
/// struct ServiceEnds {
///     ended: Vec<Line>,
/// }
///
/// impl Subscriber for ServiceEnds {
///     const NAME: &'static str = "service_ends";
///
///     async fn on_event(&mut self, event: &MonitorEvent) {
///         if let Some(line) = service_ended(event) {
///             self.ended.push(line);
///         }
///     }
/// }
///
/// /// The line that has just ended service, if this event says so.
/// fn service_ended(event: &MonitorEvent) -> Option<Line> {
///     match &event.change {
///         Change::LineStatus(change)
///             if change.previous.condition == LineCondition::Normal
///                 && change.current.condition == LineCondition::NonServiceHours =>
///         {
///             Some(change.current.line)
///         }
///         _ => None,
///     }
/// }
///
/// let status = |condition| LineStatus {
///     line: Line::KwunTong,
///     condition,
///     message: None,
/// };
/// let event = MonitorEvent {
///     observed_at: Timestamp::UNIX_EPOCH,
///     change: Change::LineStatus(LineStatusChange {
///         previous: status(LineCondition::Normal),
///         current: status(LineCondition::NonServiceHours),
///     }),
/// };
/// assert_eq!(service_ended(&event), Some(Line::KwunTong));
///
/// // The monitor calls `on_event` from the subscriber's own task.
/// let mut subscriber = ServiceEnds::default();
/// subscriber.on_event(&event).await;
/// assert_eq!(subscriber.ended, [Line::KwunTong]);
/// # }
/// ```
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
