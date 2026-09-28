use std::{fmt, sync::Arc};

use jiff::Timestamp;
use tokio::time::Instant;

use dut_core::domain::source_health::HealthState;

/// What a poller knows about its feed: the last value it read, and how well
/// the source can currently be read.
pub struct FeedState<T> {
    latest: Option<Polled<T>>,
    health: HealthState,
    attempts: u64,
}

impl<T> FeedState<T> {
    pub(crate) const fn new() -> Self {
        Self {
            latest: None,
            health: HealthState::Starting,
            attempts: 0,
        }
    }

    /// The value of the last successful poll. A failed poll leaves it in
    /// place, so it only changes when upstream answers.
    pub const fn latest(&self) -> Option<&Polled<T>> {
        self.latest.as_ref()
    }

    pub const fn health(&self) -> HealthState {
        self.health
    }

    /// How many polls have finished, successfully or not.
    pub const fn attempts(&self) -> u64 {
        self.attempts
    }

    pub(crate) fn record(&mut self, polled: Option<Polled<T>>, health: HealthState) {
        if polled.is_some() {
            self.latest = polled;
        }
        self.health = health;
        self.attempts = self.attempts.saturating_add(1);
    }
}

// Implemented by hand so that neither requires the same of `T`.
impl<T> Clone for FeedState<T> {
    fn clone(&self) -> Self {
        Self {
            latest: self.latest.clone(),
            health: self.health,
            attempts: self.attempts,
        }
    }
}

impl<T> fmt::Debug for FeedState<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FeedState")
            .field("latest", &self.latest)
            .field("health", &self.health)
            .field("attempts", &self.attempts)
            .finish()
    }
}

/// A value read by a successful poll.
///
/// The value sits behind an [`Arc`], so readers share it rather than copy
/// it, and a watcher can tell a new poll's value from the one it already
/// saw by pointer.
pub struct Polled<T> {
    value: Arc<T>,
    fetched_at: Timestamp,
    polled_at: Instant,
}

impl<T> Polled<T> {
    pub(crate) const fn new(value: Arc<T>, fetched_at: Timestamp, polled_at: Instant) -> Self {
        Self {
            value,
            fetched_at,
            polled_at,
        }
    }

    pub const fn value(&self) -> &Arc<T> {
        &self.value
    }

    /// When this service received the value from upstream.
    pub const fn fetched_at(&self) -> Timestamp {
        self.fetched_at
    }

    /// The same moment on the runtime's clock, which tests can pause.
    pub(crate) const fn polled_at(&self) -> Instant {
        self.polled_at
    }
}

// Implemented by hand so that neither requires the same of `T`.
impl<T> Clone for Polled<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            fetched_at: self.fetched_at,
            polled_at: self.polled_at,
        }
    }
}

impl<T> fmt::Debug for Polled<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Polled")
            .field("fetched_at", &self.fetched_at)
            .finish_non_exhaustive()
    }
}
