use std::{error::Error, sync::Arc};

use jiff::Timestamp;
use tokio::{
    sync::watch,
    time::{self, Instant, MissedTickBehavior},
};
use tracing::{Instrument, debug, info, info_span, warn};

use dut_core::{application::feed::Feed, domain::source_health::HealthState};
use dut_telemetry::millis;

use crate::{
    handle::FeedHandle,
    health::SourceHealth,
    schedule::Schedule,
    state::{FeedState, Polled},
};

/// Starts reading `feed` on `schedule` in a background task, and returns the
/// handle through which its latest value is read.
///
/// The task stops once every handle and subscriber is dropped. A failed poll
/// is logged and keeps the last value; polling carries on, sooner when the
/// schedule's `retry_after` is shorter than its interval. A poll that
/// overruns the interval delays the next one rather than being followed by a
/// burst of catch-up polls against a struggling upstream.
///
/// Must be called within a Tokio runtime.
///
/// # Examples
///
/// Poll weather warnings every minute and read the latest value. The
/// schedules the service runs on are constants in `src/bootstrap/config.rs`.
///
/// ```
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::time::Duration;
///
/// use dut_core::{
///     application::{feed::Feed, source::SourceUnavailable},
///     domain::{source_health::SourceId, weather::WeatherWarnings},
/// };
/// use dut_poll::Schedule;
///
/// /// The Observatory with no warning in force.
/// struct ClearSkies;
///
/// impl Feed for ClearSkies {
///     type Item = WeatherWarnings;
///
///     const SOURCE: SourceId = SourceId::HkoWarnings;
///
///     async fn fetch(&self) -> Result<WeatherWarnings, SourceUnavailable> {
///         Ok(WeatherWarnings { warnings: Vec::new() })
///     }
/// }
///
/// let schedule = Schedule {
///     interval: Duration::from_secs(60),
///     first_poll_after: Duration::ZERO,
///     retry_after: Duration::from_secs(60),
///     // The interval plus the request timeout.
///     fresh_for: Duration::from_secs(70),
///     stale_if_error: Duration::from_secs(15 * 60),
///     blind_after: Duration::from_secs(5 * 60),
/// };
///
/// let warnings = dut_poll::spawn(ClearSkies, schedule);
///
/// // Waits for the first poll, which has only just started.
/// let snapshot = warnings.snapshot().await?;
/// assert!(snapshot.value().warnings.is_empty());
/// assert!(!snapshot.freshness().is_stale());
/// # Ok(())
/// # }
/// ```
pub fn spawn<F: Feed>(feed: F, schedule: Schedule) -> FeedHandle<F::Item> {
    let (state, reader) = watch::channel(FeedState::new());
    let span = info_span!("poll", source = %F::SOURCE);
    tokio::spawn(run(feed, schedule, state).instrument(span));
    FeedHandle::new(reader, F::SOURCE, schedule)
}

async fn run<F: Feed>(feed: F, schedule: Schedule, state: watch::Sender<FeedState<F::Item>>) {
    let started_at = Instant::now();
    let mut health = SourceHealth::new(started_at, schedule.blind_after);
    let mut ticks = time::interval_at(started_at + schedule.first_poll_after, schedule.interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    info!(
        interval_ms = millis(schedule.interval),
        first_poll_after_ms = millis(schedule.first_poll_after),
        "polling started"
    );

    loop {
        tokio::select! {
            _ = ticks.tick() => {
                let succeeded = poll(&feed, &mut health, &state).await;
                if !succeeded && schedule.retry_after < schedule.interval {
                    debug!(retry_after_ms = millis(schedule.retry_after), "retrying early");
                    ticks.reset_after(schedule.retry_after);
                }
            }
            () = state.closed() => {
                info!("no readers left; polling stopped");
                return;
            }
        }
    }
}

/// Reads the feed once and publishes the outcome to readers in one update,
/// so they never see a new value with stale health or the reverse. Returns
/// whether the read succeeded.
async fn poll<F: Feed>(
    feed: &F,
    health: &mut SourceHealth,
    state: &watch::Sender<FeedState<F::Item>>,
) -> bool {
    let started_at = Instant::now();
    let result = feed.fetch().await;
    let now = Instant::now();
    let fetch_ms = millis(now - started_at);

    let (polled, change) = match result {
        Ok(value) => {
            debug!(fetch_ms, "poll succeeded");
            let polled = Polled::new(Arc::new(value), Timestamp::now(), now);
            (Some(polled), health.succeeded(now))
        }
        Err(fetch_error) => {
            let change = health.failed(now);
            warn!(
                fetch_ms,
                consecutive_failures = health.consecutive_failures(),
                error = &fetch_error as &dyn Error,
                "poll failed; keeping the last value"
            );
            (None, change)
        }
    };

    let succeeded = polled.is_some();
    let current = health.state();
    state.send_modify(|state| state.record(polled, current));
    if let Some(previous) = change {
        log_health_change(previous, current, health.consecutive_failures());
    }
    succeeded
}

fn log_health_change(previous: HealthState, current: HealthState, consecutive_failures: u32) {
    match current {
        HealthState::Healthy => info!(%previous, %current, "source is readable"),
        HealthState::Failing => warn!(
            %previous,
            %current,
            consecutive_failures,
            "source is failing; its last value still stands"
        ),
        HealthState::Blind => warn!(
            %previous,
            %current,
            consecutive_failures,
            "source is blind; its last value is too old to rely on"
        ),
        HealthState::Starting => {}
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Mutex, PoisonError,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use tokio::time::sleep;

    use dut_core::{application::source::SourceUnavailable, domain::source_health::SourceId};

    use super::*;

    /// Answers polls from a script, then repeats its last answer.
    struct ScriptedFeed {
        answers: Mutex<VecDeque<Option<u32>>>,
        calls: Arc<AtomicUsize>,
    }

    impl ScriptedFeed {
        fn new(answers: impl IntoIterator<Item = Option<u32>>) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            let feed = Self {
                answers: Mutex::new(answers.into_iter().collect()),
                calls: Arc::clone(&calls),
            };
            (feed, calls)
        }
    }

    impl Feed for ScriptedFeed {
        type Item = u32;

        const SOURCE: SourceId = SourceId::HkoWarnings;

        async fn fetch(&self) -> Result<u32, SourceUnavailable> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
            let answer = if answers.len() > 1 {
                answers.pop_front().flatten()
            } else {
                answers.front().copied().flatten()
            };
            answer.ok_or_else(|| SourceUnavailable::new("scripted failure"))
        }
    }

    const fn seconds(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    const SCHEDULE: Schedule = Schedule {
        interval: seconds(30),
        first_poll_after: Duration::ZERO,
        retry_after: seconds(30),
        fresh_for: seconds(33),
        stale_if_error: seconds(900),
        blind_after: seconds(90),
    };

    fn latest(handle: &FeedHandle<u32>) -> Option<u32> {
        handle
            .subscribe()
            .borrow()
            .latest()
            .map(|polled| **polled.value())
    }

    fn health(handle: &FeedHandle<u32>) -> HealthState {
        handle.subscribe().borrow().health()
    }

    #[tokio::test(start_paused = true)]
    async fn polls_at_once_and_then_every_interval() {
        let (feed, calls) = ScriptedFeed::new([Some(1)]);

        let handle = spawn(feed, SCHEDULE);
        sleep(seconds(1)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(latest(&handle), Some(1));

        sleep(seconds(60)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_first_poll_when_asked_to() {
        let (feed, calls) = ScriptedFeed::new([Some(1)]);
        let schedule = Schedule {
            first_poll_after: seconds(60),
            ..SCHEDULE
        };

        let handle = spawn(feed, schedule);
        sleep(seconds(59)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(handle.subscribe().borrow().attempts(), 0);

        sleep(seconds(2)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_a_failed_poll_early_then_returns_to_the_interval() {
        let (feed, calls) = ScriptedFeed::new([None, Some(1)]);
        let schedule = Schedule {
            interval: seconds(3_600),
            retry_after: seconds(60),
            ..SCHEDULE
        };

        let handle = spawn(feed, schedule);
        sleep(seconds(1)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(latest(&handle), None);

        sleep(seconds(60)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(latest(&handle), Some(1));

        sleep(seconds(3_598)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        sleep(seconds(2)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_poll_keeps_the_last_value() {
        let (feed, _) = ScriptedFeed::new([Some(1), None]);

        let handle = spawn(feed, SCHEDULE);
        sleep(seconds(31)).await;

        assert_eq!(latest(&handle), Some(1));
        assert_eq!(health(&handle), HealthState::Failing);
        assert_eq!(handle.subscribe().borrow().attempts(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn health_follows_the_polls() {
        let (feed, _) = ScriptedFeed::new([Some(1), None, None, None, Some(2)]);

        let handle = spawn(feed, SCHEDULE);
        sleep(seconds(1)).await;
        assert_eq!(health(&handle), HealthState::Healthy);

        sleep(seconds(30)).await;
        assert_eq!(health(&handle), HealthState::Failing);

        sleep(seconds(60)).await;
        assert_eq!(health(&handle), HealthState::Blind);
        assert_eq!(latest(&handle), Some(1));

        sleep(seconds(30)).await;
        assert_eq!(health(&handle), HealthState::Healthy);
        assert_eq!(latest(&handle), Some(2));
    }

    #[tokio::test(start_paused = true)]
    async fn polling_stops_once_every_reader_is_dropped() {
        let (feed, calls) = ScriptedFeed::new([Some(1)]);

        let handle = spawn(feed, SCHEDULE);
        sleep(seconds(1)).await;
        drop(handle);
        sleep(seconds(120)).await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
