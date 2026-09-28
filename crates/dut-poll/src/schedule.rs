use std::time::Duration;

use dut_core::application::source::Freshness;

/// When a feed is read, and how long what was read stays usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Schedule {
    /// Time between the starts of consecutive polls.
    pub interval: Duration,
    /// Delay before the first poll; zero polls as soon as polling starts.
    pub first_poll_after: Duration,
    /// Delay before the next poll after a failed one. A feed read once a day
    /// retries within minutes, so one failure does not leave it without data
    /// until the next day; a value at or above `interval` keeps the regular
    /// schedule.
    pub retry_after: Duration,
    /// How long a value counts as fresh after it was read. The interval plus
    /// the request timeout keeps a value fresh while the next poll is still
    /// in flight.
    pub fresh_for: Duration,
    /// How long after it stops being fresh a value may still be served,
    /// marked stale, while polls keep failing.
    pub stale_if_error: Duration,
    /// How long without a successful poll before the source counts as blind.
    pub blind_after: Duration,
}

impl Schedule {
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

    const SCHEDULE: Schedule = Schedule {
        interval: seconds(30),
        first_poll_after: Duration::ZERO,
        retry_after: seconds(30),
        fresh_for: seconds(33),
        stale_if_error: seconds(900),
        blind_after: seconds(120),
    };

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
