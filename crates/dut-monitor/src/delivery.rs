use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

use dut_core::{application::subscriber::Subscriber, domain::event::MonitorEvent};

/// Hands each received event to `subscriber`, one at a time, until the
/// monitor stops publishing.
pub(crate) async fn deliver<S: Subscriber>(
    mut events: broadcast::Receiver<MonitorEvent>,
    mut subscriber: S,
) {
    loop {
        match events.recv().await {
            Ok(event) => subscriber.on_event(&event).await,
            Err(RecvError::Lagged(missed)) => {
                warn!(missed, "subscriber fell behind; events were dropped");
                subscriber.on_lagged(missed).await;
            }
            Err(RecvError::Closed) => {
                info!("monitor stopped publishing; subscriber finished");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use jiff::Timestamp;
    use tokio::task::yield_now;

    use dut_core::domain::{
        event::Change,
        source_health::{HealthChange, HealthState, SourceId},
    };

    use super::*;

    /// What a subscriber was handed, in order.
    #[derive(Debug, Eq, PartialEq)]
    enum Delivered {
        Event(HealthState),
        Lagged(u64),
    }

    /// Records every delivery into a shared log the test can read.
    struct Recorder(Arc<Mutex<Vec<Delivered>>>);

    impl Recorder {
        fn record(&self, delivered: Delivered) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(delivered);
        }
    }

    impl Subscriber for Recorder {
        const NAME: &'static str = "recorder";

        async fn on_event(&mut self, event: &MonitorEvent) {
            if let Change::SourceHealth(change) = &event.change {
                self.record(Delivered::Event(change.current));
            }
        }

        async fn on_lagged(&mut self, missed: u64) {
            self.record(Delivered::Lagged(missed));
        }
    }

    /// An event told apart by its health state.
    fn event(current: HealthState) -> MonitorEvent {
        MonitorEvent {
            observed_at: Timestamp::UNIX_EPOCH,
            change: Change::SourceHealth(HealthChange {
                source: SourceId::MtrLineStatus,
                previous: HealthState::Starting,
                current,
            }),
        }
    }

    fn start(capacity: usize) -> (broadcast::Sender<MonitorEvent>, Arc<Mutex<Vec<Delivered>>>) {
        let (events, receiver) = broadcast::channel(capacity);
        let log = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(deliver(receiver, Recorder(Arc::clone(&log))));
        (events, log)
    }

    fn delivered(log: &Mutex<Vec<Delivered>>) -> Vec<Delivered> {
        std::mem::take(&mut *log.lock().unwrap_or_else(PoisonError::into_inner))
    }

    #[tokio::test]
    async fn delivers_events_in_the_order_they_were_published() {
        let (events, log) = start(16);

        for state in [
            HealthState::Healthy,
            HealthState::Failing,
            HealthState::Blind,
        ] {
            events
                .send(event(state))
                .expect("the subscriber is listening");
        }
        yield_now().await;

        assert_eq!(
            delivered(&log),
            [
                Delivered::Event(HealthState::Healthy),
                Delivered::Event(HealthState::Failing),
                Delivered::Event(HealthState::Blind),
            ]
        );
    }

    #[tokio::test]
    async fn reports_events_dropped_while_the_subscriber_fell_behind() {
        let (events, log) = start(2);

        // The subscriber's task cannot run until the test yields, so it
        // falls behind by the three events the channel cannot hold.
        for state in [
            HealthState::Starting,
            HealthState::Healthy,
            HealthState::Failing,
            HealthState::Blind,
            HealthState::Healthy,
        ] {
            events
                .send(event(state))
                .expect("the subscriber is listening");
        }
        yield_now().await;

        assert_eq!(
            delivered(&log),
            [
                Delivered::Lagged(3),
                Delivered::Event(HealthState::Blind),
                Delivered::Event(HealthState::Healthy),
            ]
        );
    }

    #[tokio::test]
    async fn finishes_once_the_monitor_stops_publishing() {
        let (events, receiver) = broadcast::channel(4);
        let log = Arc::new(Mutex::new(Vec::new()));
        let task = tokio::spawn(deliver(receiver, Recorder(Arc::clone(&log))));

        drop(events);

        task.await.expect("delivery should end without panicking");
    }
}
