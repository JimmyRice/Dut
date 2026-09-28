use std::{mem, time::Duration};

use tokio::time::Instant;

use dut_core::domain::source_health::HealthState;

/// Derives one source's [`HealthState`] from the outcome of each poll.
#[derive(Debug)]
pub(crate) struct SourceHealth {
    blind_after: Duration,
    state: HealthState,
    /// When the source was last read, or when polling started if it never
    /// was: a source that never answers turns blind as one that stopped does.
    last_seen: Instant,
    consecutive_failures: u32,
}

impl SourceHealth {
    pub(crate) const fn new(started_at: Instant, blind_after: Duration) -> Self {
        Self {
            blind_after,
            state: HealthState::Starting,
            last_seen: started_at,
            consecutive_failures: 0,
        }
    }

    pub(crate) const fn state(&self) -> HealthState {
        self.state
    }

    pub(crate) const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Records a successful poll. Returns the previous state if it changed.
    pub(crate) fn succeeded(&mut self, now: Instant) -> Option<HealthState> {
        self.last_seen = now;
        self.consecutive_failures = 0;
        self.transition(HealthState::Healthy)
    }

    /// Records a failed poll. Returns the previous state if it changed.
    pub(crate) fn failed(&mut self, now: Instant) -> Option<HealthState> {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let unseen_for = now.saturating_duration_since(self.last_seen);
        let next = if unseen_for >= self.blind_after {
            HealthState::Blind
        } else {
            HealthState::Failing
        };
        self.transition(next)
    }

    fn transition(&mut self, next: HealthState) -> Option<HealthState> {
        (next != self.state).then(|| mem::replace(&mut self.state, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLIND_AFTER: Duration = Duration::from_secs(120);

    #[test]
    fn a_source_turns_blind_once_unseen_for_too_long() {
        let start = Instant::now();
        let mut health = SourceHealth::new(start, BLIND_AFTER);

        assert_eq!(health.succeeded(start), Some(HealthState::Starting));
        assert_eq!(
            health.failed(start + Duration::from_secs(30)),
            Some(HealthState::Healthy)
        );
        assert_eq!(health.state(), HealthState::Failing);
        assert_eq!(health.failed(start + Duration::from_secs(60)), None);
        assert_eq!(
            health.failed(start + BLIND_AFTER),
            Some(HealthState::Failing)
        );
        assert_eq!(health.state(), HealthState::Blind);
        assert_eq!(health.consecutive_failures(), 3);
    }

    #[test]
    fn a_success_restores_a_blind_source() {
        let start = Instant::now();
        let mut health = SourceHealth::new(start, BLIND_AFTER);
        health.failed(start + BLIND_AFTER);

        assert_eq!(
            health.succeeded(start + BLIND_AFTER * 2),
            Some(HealthState::Blind)
        );
        assert_eq!(health.state(), HealthState::Healthy);
        assert_eq!(health.consecutive_failures(), 0);
    }

    #[test]
    fn a_source_that_never_answers_is_failing_then_blind() {
        let start = Instant::now();
        let mut health = SourceHealth::new(start, BLIND_AFTER);

        assert_eq!(health.failed(start), Some(HealthState::Starting));
        assert_eq!(health.state(), HealthState::Failing);
        health.failed(start + BLIND_AFTER);
        assert_eq!(health.state(), HealthState::Blind);
    }
}
