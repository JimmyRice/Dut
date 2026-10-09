use std::time::Duration;

use dut_core::application::source::Freshness;

/// When a feed is read, and how long what was read stays usable.
///
/// Built in `const` items, starting from [`every`](Self::every): each step
/// checks its own value, so a schedule the poller cannot run, such as one
/// with a zero interval, fails the build.
///
/// ```
/// use std::time::Duration;
///
/// use dut_poll::Schedule;
///
/// const WARNINGS: Schedule = Schedule::every(Duration::from_secs(60))
///     // The interval plus the request timeout.
///     .fresh_for(Duration::from_secs(65))
///     .stale_if_error(Duration::from_secs(15 * 60))
///     .blind_after(Duration::from_secs(5 * 60));
/// ```
///
/// A value that would go stale before the next poll does not compile:
///
/// ```compile_fail
/// use std::time::Duration;
///
/// use dut_poll::Schedule;
///
/// const WARNINGS: Schedule =
///     Schedule::every(Duration::from_secs(60)).fresh_for(Duration::from_secs(30));
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Schedule {
    pub(crate) interval: Duration,
    pub(crate) first_poll_after: Duration,
    pub(crate) retry_after: Duration,
    pub(crate) fresh_for: Duration,
    pub(crate) stale_if_error: Duration,
    pub(crate) blind_after: Duration,
}

impl Schedule {
    /// Polls every `interval`, measured between the starts of consecutive
    /// polls, beginning as soon as polling starts.
    ///
    /// Until the other steps say otherwise, a failed poll waits the full
    /// interval, a value is fresh for one interval and never served stale,
    /// and the source never counts as blind.
    pub const fn every(interval: Duration) -> Self {
        assert!(!interval.is_zero(), "a poll interval must be positive");
        Self {
            interval,
            first_poll_after: Duration::ZERO,
            retry_after: interval,
            fresh_for: interval,
            stale_if_error: Duration::ZERO,
            blind_after: Duration::MAX,
        }
    }

    /// Delays the first poll; zero polls as soon as polling starts.
    #[must_use]
    pub const fn first_poll_after(mut self, delay: Duration) -> Self {
        self.first_poll_after = delay;
        self
    }

    /// Delay before the next poll after a failed one. A feed read once a day
    /// retries within minutes, so one failure does not leave it without data
    /// until the next day; a value at or above the interval keeps the
    /// regular schedule.
    #[must_use]
    pub const fn retry_after(mut self, delay: Duration) -> Self {
        assert!(
            !delay.is_zero(),
            "a retry delay must be positive, or failures would retry in a tight loop"
        );
        self.retry_after = delay;
        self
    }

    /// How long a value counts as fresh after it was read. The interval plus
    /// the request timeout keeps a value fresh while the next poll is still
    /// in flight; less would turn every value stale before its successor.
    #[must_use]
    pub const fn fresh_for(mut self, period: Duration) -> Self {
        assert!(
            period.as_nanos() >= self.interval.as_nanos(),
            "a value must stay fresh for at least one poll interval"
        );
        self.fresh_for = period;
        self
    }

    /// How long after it stops being fresh a value may still be served,
    /// marked stale, while polls keep failing.
    #[must_use]
    pub const fn stale_if_error(mut self, period: Duration) -> Self {
        self.stale_if_error = period;
        self
    }

    /// How long without a successful poll before the source counts as blind.
    #[must_use]
    pub const fn blind_after(mut self, period: Duration) -> Self {
        self.blind_after = period;
        self
    }

    /// The freshness of a value read `age` ago, or `None` once it is too old
    /// to serve at all.
    pub(crate) fn freshness(&self, age: Duration) -> Option<Freshness> {
        if age < self.fresh_for {
            Some(Freshness::Fresh {
                expires_in: self.fresh_for - age,
            })
        } else if age < self.fresh_for.saturating_add(self.stale_if_error) {
            Some(Freshness::Stale)
        } else {
            None
        }
    }

    /// How long a reader waits for the first poll to finish. The first poll
    /// is already on its way, and answering without it would fail every
    /// request made just after startup.
    pub(crate) const fn first_poll_wait(&self) -> Duration {
        self.first_poll_after.saturating_add(self.fresh_for)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn seconds(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    const SCHEDULE: Schedule = Schedule::every(seconds(30))
        .fresh_for(seconds(33))
        .stale_if_error(seconds(900))
        .blind_after(seconds(120));

    #[test]
    fn a_value_is_fresh_until_the_next_poll_is_overdue() {
        assert_eq!(
            SCHEDULE.freshness(seconds(10)),
            Some(Freshness::Fresh {
                expires_in: seconds(23)
            })
        );
    }

    #[test]
    fn an_overdue_value_is_stale_within_the_error_window() {
        assert_eq!(SCHEDULE.freshness(seconds(33)), Some(Freshness::Stale));
        assert_eq!(SCHEDULE.freshness(seconds(932)), Some(Freshness::Stale));
    }

    #[test]
    fn a_value_past_the_error_window_is_not_served() {
        assert_eq!(SCHEDULE.freshness(seconds(933)), None);
    }
}
