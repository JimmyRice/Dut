use std::sync::Arc;

use jiff::Timestamp;
use tokio::sync::broadcast;
use tracing::info;

use dut_core::domain::{
    event::{Change, MonitorEvent},
    source_health::{HealthChange, HealthState},
};
use dut_poll::FeedHandle;

/// Publishes what changed in one feed after each of its polls.
///
/// `diff` compares the previous successful value with the current one. It
/// only runs when a poll brought a new value, so a failed poll never looks
/// like a change in the data. Runs until the feed's poller stops.
pub(crate) async fn watch<T, D>(
    feed: FeedHandle<T>,
    events: broadcast::Sender<MonitorEvent>,
    diff: D,
) where
    T: Send + Sync + 'static,
    D: Fn(&T, &T) -> Vec<Change>,
{
    let source = feed.source();
    let mut state = feed.subscribe();
    let mut health = HealthState::Starting;
    let mut seen: Option<Arc<T>> = None;

    while state.changed().await.is_ok() {
        let (latest, current_health) = {
            let current = state.borrow_and_update();
            let latest = current.latest().map(|polled| Arc::clone(polled.value()));
            (latest, current.health())
        };
        let mut changes = Vec::new();

        if current_health != health {
            changes.push(Change::SourceHealth(HealthChange {
                source,
                previous: health,
                current: current_health,
            }));
            health = current_health;
        }
        if let Some(latest) = latest {
            match &seen {
                None => info!(%source, "baseline read; later changes will be published"),
                Some(previous) if !Arc::ptr_eq(previous, &latest) => {
                    changes.extend(diff(previous, &latest));
                }
                Some(_) => {}
            }
            seen = Some(latest);
        }

        let observed_at = Timestamp::now();
        for change in changes {
            // The event log always subscribes, so a send only fails once the
            // process is shutting down.
            let _ = events.send(MonitorEvent {
                observed_at,
                change,
            });
        }
    }
}
