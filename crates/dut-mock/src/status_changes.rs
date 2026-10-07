//! The line status changes a scenario puts the feed through, for the mock
//! event stream.

use std::iter::Cycle;
use std::vec::IntoIter;

use jiff::Timestamp;

use dut_core::{application::source::SourceUnavailable, domain::line_status::LineStatusChange};

use crate::{network_status, scenario::StatusScenario, seed::Seed};

/// The changes in the line status feed as a scenario unfolds: the incident
/// is reported, then service returns to normal, and so on for as long as the
/// client listens.
///
/// The batches come from the same simulated feeds the mock status route
/// serves, compared with the domain's own diff, so an event describes
/// exactly what polling the status route before and after would show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimulatedStatusChanges {
    scenario: StatusScenario,
    seed: Seed,
}

impl SimulatedStatusChanges {
    pub const fn new(scenario: StatusScenario, seed: Seed) -> Self {
        Self { scenario, seed }
    }

    /// The changes the feed goes through, one batch for each moment the MTR
    /// would republish it, repeating forever. A line changes at most once per
    /// batch.
    ///
    /// The iterator is empty when nothing changes: good service stays good,
    /// a stale feed never updates, and an unreachable one is a matter for
    /// source health, which this stream does not carry.
    pub fn batches(&self) -> Result<Cycle<IntoIter<Vec<LineStatusChange>>>, SourceUnavailable> {
        if self.scenario == StatusScenario::UpstreamUnavailable {
            return Ok(Vec::new().into_iter().cycle());
        }
        let now = Timestamp::now();
        let calm = network_status::simulate(StatusScenario::Normal, self.seed, now)?;
        let unsettled = network_status::simulate(self.scenario, self.seed, now)?;
        let onset = unsettled.value().changes_since(calm.value());
        let recovery = calm.value().changes_since(unsettled.value());

        let batches = if onset.is_empty() {
            Vec::new()
        } else {
            vec![onset, recovery]
        };
        Ok(batches.into_iter().cycle())
    }
}

#[cfg(test)]
mod tests {
    use dut_core::domain::line_status::LineCondition;

    use super::*;

    fn cycle(scenario: StatusScenario, seed: u64) -> Vec<Vec<LineStatusChange>> {
        SimulatedStatusChanges::new(scenario, Seed::new(seed))
            .batches()
            .expect("the scenario should simulate")
            .take(4)
            .collect()
    }

    #[test]
    fn an_incident_is_reported_and_then_recovers() {
        let batches = cycle(StatusScenario::Delayed, 7);

        let [onset] = batches[0].as_slice() else {
            panic!("one line should be delayed: {:?}", batches[0]);
        };
        assert_eq!(onset.previous.condition, LineCondition::Normal);
        assert_eq!(onset.current.condition, LineCondition::Delayed);
        assert!(onset.current.message.is_some());

        let [recovery] = batches[1].as_slice() else {
            panic!("the same line should recover: {:?}", batches[1]);
        };
        assert_eq!(recovery.current.line, onset.current.line);
        assert_eq!(recovery.previous.condition, LineCondition::Delayed);
        assert_eq!(recovery.current.condition, LineCondition::Normal);
        assert_eq!(recovery.current.message, None);
    }

    #[test]
    fn the_cycle_repeats() {
        let batches = cycle(StatusScenario::Disrupted, 3);

        assert_eq!(batches[0], batches[2]);
        assert_eq!(batches[1], batches[3]);
    }

    #[test]
    fn a_seed_picks_the_same_line_every_time() {
        assert_eq!(
            cycle(StatusScenario::Delayed, 5),
            cycle(StatusScenario::Delayed, 5)
        );
    }

    #[test]
    fn a_network_wide_condition_changes_every_line() {
        for scenario in [
            StatusScenario::TyphoonSignal,
            StatusScenario::NonServiceHours,
        ] {
            let batches = cycle(scenario, 1);

            assert_eq!(batches[0].len(), 11, "{scenario:?}");
            assert_eq!(batches[1].len(), 11, "{scenario:?}");
        }
    }

    #[test]
    fn scenarios_without_a_change_stay_silent() {
        for scenario in [
            StatusScenario::Normal,
            StatusScenario::Stale,
            StatusScenario::UpstreamUnavailable,
        ] {
            assert!(cycle(scenario, 1).is_empty(), "{scenario:?}");
        }
    }
}
