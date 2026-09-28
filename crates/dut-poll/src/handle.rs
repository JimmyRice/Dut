use std::{fmt, sync::Arc, time::Duration};

use thiserror::Error;
use tokio::{
    sync::watch,
    time::{self, Instant},
};
use tracing::debug;

use dut_core::{
    application::source::{Snapshot, SourceUnavailable},
    domain::source_health::SourceId,
};
use dut_telemetry::millis;

use crate::{schedule::Schedule, state::FeedState};

/// Reads what a poller last read from its feed. Cloning is cheap.
pub struct FeedHandle<T> {
    state: watch::Receiver<FeedState<T>>,
    source: SourceId,
    schedule: Schedule,
}

impl<T> FeedHandle<T> {
    pub(crate) const fn new(
        state: watch::Receiver<FeedState<T>>,
        source: SourceId,
        schedule: Schedule,
    ) -> Self {
        Self {
            state,
            source,
            schedule,
        }
    }

    /// Which source the feed reads.
    pub const fn source(&self) -> SourceId {
        self.source
    }

    /// A receiver that is notified after every poll, successful or not.
    ///
    /// Borrow its value only briefly: a poll cannot publish while a borrow
    /// is held.
    pub fn subscribe(&self) -> watch::Receiver<FeedState<T>> {
        self.state.clone()
    }

    /// The last value read, with its freshness as of now.
    ///
    /// Before the first poll finishes this waits for it, within the
    /// schedule's limit. It fails when no poll has succeeded, or when the
    /// last success is too old to serve even as stale.
    pub async fn snapshot(&self) -> Result<Snapshot<T>, SourceUnavailable> {
        let mut state = self.state.clone();
        let first_poll = state.wait_for(|state| state.attempts() > 0);
        if time::timeout(self.schedule.first_poll_wait(), first_poll)
            .await
            .is_err()
        {
            debug!(source = %self.source, "gave up waiting for the first poll");
        }

        let latest = state.borrow().latest().cloned();
        let polled = latest.ok_or(Unavailable::NeverRead { feed: self.source })?;
        let age = Instant::now().saturating_duration_since(polled.polled_at());
        let freshness = self.schedule.freshness(age).ok_or_else(|| {
            debug!(source = %self.source, age_ms = millis(age), "last value is too old to serve");
            Unavailable::TooOld {
                feed: self.source,
                age,
            }
        })?;

        Ok(Snapshot::new(
            Arc::clone(polled.value()),
            polled.fetched_at(),
            freshness,
        ))
    }
}

// Implemented by hand so that neither requires the same of `T`.
impl<T> Clone for FeedHandle<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            source: self.source,
            schedule: self.schedule,
        }
    }
}

impl<T> fmt::Debug for FeedHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FeedHandle")
            .field("source", &self.source)
            .field("schedule", &self.schedule)
            .finish_non_exhaustive()
    }
}

/// Why a feed's value cannot be served.
#[derive(Debug, Error)]
enum Unavailable {
    #[error("{feed} has not been read successfully")]
    NeverRead { feed: SourceId },

    #[error("{feed} was last read {age:?} ago, too long ago to serve")]
    TooOld { feed: SourceId, age: Duration },
}

impl From<Unavailable> for SourceUnavailable {
    fn from(unavailable: Unavailable) -> Self {
        Self::new(unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::pending,
        sync::atomic::{AtomicBool, Ordering},
    };

    use tokio::time::sleep;

    use dut_core::application::{feed::Feed, source::Freshness};

    use super::*;
    use crate::poller::spawn;

    /// Answers with its value while `up` is set, and fails otherwise.
    struct SwitchableFeed {
        up: Arc<AtomicBool>,
    }

    impl Feed for SwitchableFeed {
        type Item = u32;

        const SOURCE: SourceId = SourceId::MtrLineStatus;

        async fn fetch(&self) -> Result<u32, SourceUnavailable> {
            if self.up.load(Ordering::SeqCst) {
                Ok(7)
            } else {
                Err(SourceUnavailable::new("switched off"))
            }
        }
    }

    /// Never answers, like an upstream that hangs.
    struct HangingFeed;

    impl Feed for HangingFeed {
        type Item = u32;

        const SOURCE: SourceId = SourceId::MtrLineStatus;

        async fn fetch(&self) -> Result<u32, SourceUnavailable> {
            pending().await
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
        blind_after: seconds(120),
    };

    fn switchable(up: bool) -> (FeedHandle<u32>, Arc<AtomicBool>) {
        let up = Arc::new(AtomicBool::new(up));
        let handle = spawn(
            SwitchableFeed {
                up: Arc::clone(&up),
            },
            SCHEDULE,
        );
        (handle, up)
    }

    #[tokio::test(start_paused = true)]
    async fn serves_the_first_poll_as_soon_as_it_finishes() {
        let (handle, _) = switchable(true);

        let snapshot = handle.snapshot().await.expect("first poll succeeded");

        assert_eq!(*snapshot.value(), 7);
        assert_eq!(
            snapshot.freshness(),
            Freshness::Fresh {
                expires_in: seconds(33)
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn fails_at_once_when_the_first_poll_fails() {
        let (handle, _) = switchable(false);

        let started_at = Instant::now();
        let result = handle.snapshot().await;

        assert!(result.is_err());
        assert!(started_at.elapsed() < seconds(1));
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_on_a_first_poll_that_never_finishes() {
        let handle = spawn(HangingFeed, SCHEDULE);

        let result = handle.snapshot().await;

        assert!(result.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn serves_the_last_value_as_stale_while_polls_fail() {
        let (handle, up) = switchable(true);
        handle.snapshot().await.expect("first poll succeeded");
        up.store(false, Ordering::SeqCst);

        sleep(seconds(40)).await;
        let snapshot = handle.snapshot().await.expect("stale value is served");

        assert_eq!(*snapshot.value(), 7);
        assert_eq!(snapshot.freshness(), Freshness::Stale);
    }

    #[tokio::test(start_paused = true)]
    async fn stops_serving_once_the_last_value_is_too_old() {
        let (handle, up) = switchable(true);
        handle.snapshot().await.expect("first poll succeeded");
        up.store(false, Ordering::SeqCst);

        sleep(seconds(33 + 900)).await;

        assert!(handle.snapshot().await.is_err());
    }
}
