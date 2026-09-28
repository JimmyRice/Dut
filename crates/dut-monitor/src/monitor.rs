use tokio::sync::broadcast;
use tracing::{Instrument, info_span};

use dut_core::{
    application::subscriber::Subscriber,
    domain::{
        event::{Change, MonitorEvent},
        line_status::NetworkStatus,
        next_train::NextTrainSignals,
        weather::WeatherWarnings,
    },
};
use dut_poll::FeedHandle;

use crate::{delivery::deliver, event_log::EventLog, watcher::watch};

/// How far a subscriber may fall behind before it misses events. Events are
/// rare, so this only matters for a subscriber that has stopped reading.
const EVENT_CAPACITY: usize = 256;

/// Subscribes to the monitor's events. Cloning is cheap.
#[derive(Clone, Debug)]
pub struct MonitorHandle {
    events: broadcast::Sender<MonitorEvent>,
}

impl MonitorHandle {
    /// Runs `subscriber` in its own task, handing it every event published
    /// from now on, one at a time and in order.
    ///
    /// Earlier events are not replayed, so attach subscribers while wiring
    /// the application. The task runs as long as the monitor does. Must be
    /// called within a Tokio runtime.
    pub fn attach<S: Subscriber>(&self, subscriber: S) {
        let span = info_span!("subscriber", subscriber = S::NAME);
        tokio::spawn(deliver(self.subscribe(), subscriber).instrument(span));
    }

    /// Receives every event published from now on, for a caller that runs
    /// its own receive loop. Prefer [`attach`](Self::attach), which handles
    /// lagging and shutdown.
    ///
    /// Earlier events are not replayed. A receiver that falls behind gets
    /// `RecvError::Lagged` and should resynchronise from the feeds' latest
    /// values.
    pub fn subscribe(&self) -> broadcast::Receiver<MonitorEvent> {
        self.events.subscribe()
    }
}

/// Starts watching every polled feed, one task per feed, and returns the
/// handle to subscribe to their changes. The event log is attached before
/// any event can be published.
///
/// The tasks run as long as the feeds are polled. Must be called within a
/// Tokio runtime.
pub fn spawn(
    line_status: &FeedHandle<NetworkStatus>,
    weather_warnings: &FeedHandle<WeatherWarnings>,
    next_train_signals: &FeedHandle<NextTrainSignals>,
) -> MonitorHandle {
    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let handle = MonitorHandle { events };
    handle.attach(EventLog);
    let events = &handle.events;

    spawn_watch(line_status, events, |previous, current| {
        wrap(current.changes_since(previous), Change::LineStatus)
    });
    spawn_watch(weather_warnings, events, |previous, current| {
        wrap(current.changes_since(previous), Change::WeatherWarning)
    });
    spawn_watch(next_train_signals, events, |previous, current| {
        wrap(current.changes_since(previous), Change::NextTrainSignal)
    });

    handle
}

fn spawn_watch<T, D>(feed: &FeedHandle<T>, events: &broadcast::Sender<MonitorEvent>, diff: D)
where
    T: Send + Sync + 'static,
    D: Fn(&T, &T) -> Vec<Change> + Send + 'static,
{
    let span = info_span!("monitor", source = %feed.source());
    tokio::spawn(watch(feed.clone(), events.clone(), diff).instrument(span));
}

fn wrap<C>(changes: Vec<C>, into: fn(C) -> Change) -> Vec<Change> {
    changes.into_iter().map(into).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Mutex, PoisonError},
        time::Duration,
    };

    use jiff::Timestamp;
    use tokio::{sync::broadcast::error::TryRecvError, time::sleep};

    use dut_core::{
        application::{feed::Feed, source::SourceUnavailable},
        domain::{
            line_status::{LineCondition, LineStatus, LineStatusChange},
            network::Line,
            source_health::{HealthChange, HealthState, SourceId},
            weather::{ActiveWarning, CycloneSignal, WarningChange, WeatherWarning},
        },
    };
    use dut_poll::Schedule;

    use super::*;

    /// Answers polls from a script, then repeats its last answer. `None`
    /// fails the poll.
    struct ScriptedFeed<T> {
        answers: Mutex<VecDeque<Option<T>>>,
    }

    impl<T: Clone> ScriptedFeed<T> {
        fn new(answers: impl IntoIterator<Item = Option<T>>) -> Self {
            Self {
                answers: Mutex::new(answers.into_iter().collect()),
            }
        }

        fn next(&self) -> Result<T, SourceUnavailable> {
            let mut answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
            let answer = if answers.len() > 1 {
                answers.pop_front().flatten()
            } else {
                answers.front().cloned().flatten()
            };
            answer.ok_or_else(|| SourceUnavailable::new("scripted failure"))
        }
    }

    impl Feed for ScriptedFeed<NetworkStatus> {
        type Item = NetworkStatus;
        const SOURCE: SourceId = SourceId::MtrLineStatus;

        async fn fetch(&self) -> Result<NetworkStatus, SourceUnavailable> {
            self.next()
        }
    }

    impl Feed for ScriptedFeed<WeatherWarnings> {
        type Item = WeatherWarnings;
        const SOURCE: SourceId = SourceId::HkoWarnings;

        async fn fetch(&self) -> Result<WeatherWarnings, SourceUnavailable> {
            self.next()
        }
    }

    impl Feed for ScriptedFeed<NextTrainSignals> {
        type Item = NextTrainSignals;
        const SOURCE: SourceId = SourceId::MtrNextTrain;

        async fn fetch(&self) -> Result<NextTrainSignals, SourceUnavailable> {
            self.next()
        }
    }

    const INTERVAL: Duration = Duration::from_secs(30);

    const SCHEDULE: Schedule = Schedule {
        interval: INTERVAL,
        first_poll_after: Duration::ZERO,
        fresh_for: Duration::from_secs(33),
        stale_if_error: Duration::from_secs(900),
        blind_after: Duration::from_secs(120),
    };

    fn kwun_tong(condition: LineCondition) -> NetworkStatus {
        NetworkStatus {
            updated_at: Timestamp::UNIX_EPOCH,
            lines: vec![LineStatus {
                line: Line::KwunTong,
                condition,
                message: None,
            }],
        }
    }

    /// Starts the monitor over scripted feeds and subscribes to it before
    /// the first poll.
    fn monitor(
        line_status: impl IntoIterator<Item = Option<NetworkStatus>>,
        weather: impl IntoIterator<Item = Option<WeatherWarnings>>,
    ) -> broadcast::Receiver<MonitorEvent> {
        let line_status = dut_poll::spawn(ScriptedFeed::new(line_status), SCHEDULE);
        let weather = dut_poll::spawn(ScriptedFeed::new(weather), SCHEDULE);
        let signals = dut_poll::spawn(
            ScriptedFeed::new([Some(NextTrainSignals::default())]),
            SCHEDULE,
        );
        spawn(&line_status, &weather, &signals).subscribe()
    }

    fn received(events: &mut broadcast::Receiver<MonitorEvent>) -> Vec<Change> {
        let mut changes = Vec::new();
        loop {
            match events.try_recv() {
                Ok(event) => changes.push(event.change),
                Err(TryRecvError::Empty) => return changes,
                Err(other) => panic!("unexpected receive error: {other}"),
            }
        }
    }

    fn data_changes(changes: Vec<Change>) -> Vec<Change> {
        changes
            .into_iter()
            .filter(|change| !matches!(change, Change::SourceHealth(_)))
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_poll_is_a_baseline() {
        let mut events = monitor(
            [Some(kwun_tong(LineCondition::Disrupted))],
            [Some(WeatherWarnings::default())],
        );

        sleep(Duration::from_secs(1)).await;
        let changes = received(&mut events);

        assert!(data_changes(changes.clone()).is_empty());
        assert!(changes.contains(&Change::SourceHealth(HealthChange {
            source: SourceId::MtrLineStatus,
            previous: HealthState::Starting,
            current: HealthState::Healthy,
        })));
    }

    #[tokio::test(start_paused = true)]
    async fn the_end_of_service_is_published() {
        let mut events = monitor(
            [
                Some(kwun_tong(LineCondition::Normal)),
                Some(kwun_tong(LineCondition::NonServiceHours)),
            ],
            [Some(WeatherWarnings::default())],
        );

        sleep(INTERVAL + Duration::from_secs(1)).await;

        assert_eq!(
            data_changes(received(&mut events)),
            [Change::LineStatus(LineStatusChange {
                previous: kwun_tong(LineCondition::Normal).lines[0].clone(),
                current: kwun_tong(LineCondition::NonServiceHours).lines[0].clone(),
            })]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_poll_is_only_a_health_change() {
        let mut events = monitor(
            [Some(kwun_tong(LineCondition::Delayed)), None],
            [Some(WeatherWarnings::default())],
        );
        sleep(Duration::from_secs(1)).await;
        received(&mut events);

        sleep(INTERVAL).await;

        assert_eq!(
            received(&mut events),
            [Change::SourceHealth(HealthChange {
                source: SourceId::MtrLineStatus,
                previous: HealthState::Healthy,
                current: HealthState::Failing,
            })]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_recovered_source_publishes_what_changed_while_it_was_away() {
        let mut events = monitor(
            [
                Some(kwun_tong(LineCondition::Delayed)),
                None,
                Some(kwun_tong(LineCondition::Normal)),
            ],
            [Some(WeatherWarnings::default())],
        );
        sleep(INTERVAL + Duration::from_secs(1)).await;
        received(&mut events);

        sleep(INTERVAL).await;
        let changes = received(&mut events);

        assert_eq!(
            changes,
            [
                Change::SourceHealth(HealthChange {
                    source: SourceId::MtrLineStatus,
                    previous: HealthState::Failing,
                    current: HealthState::Healthy,
                }),
                Change::LineStatus(LineStatusChange {
                    previous: kwun_tong(LineCondition::Delayed).lines[0].clone(),
                    current: kwun_tong(LineCondition::Normal).lines[0].clone(),
                }),
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_weather_warning_is_published() {
        let signal_no_8 = ActiveWarning {
            warning: WeatherWarning::TropicalCyclone(CycloneSignal::Eight(
                dut_core::domain::weather::Quadrant::SouthEast,
            )),
            updated_at: None,
        };
        let mut events = monitor(
            [Some(kwun_tong(LineCondition::Normal))],
            [
                Some(WeatherWarnings::default()),
                Some(WeatherWarnings {
                    warnings: vec![signal_no_8.clone()],
                }),
            ],
        );

        sleep(INTERVAL + Duration::from_secs(1)).await;

        assert_eq!(
            data_changes(received(&mut events)),
            [Change::WeatherWarning(WarningChange::Issued(signal_no_8))]
        );
    }
}
