//! What the mock API calls: Next Train boards and line status, simulated in
//! one scenario and one world.

use std::{sync::Arc, time::Duration};

use jiff::Timestamp;
use thiserror::Error;

use dut_core::{
    application::{
        next_train::{BoardView, NextTrainError, NextTrainService, NextTrainSource, StationBoards},
        source::{Freshness, Snapshot, SourceUnavailable},
    },
    domain::{
        line_status::NetworkStatus,
        network::{Line, StationCode},
        next_train::NextTrainBoard,
    },
};

use crate::{
    board::{self, Conditions},
    network_status,
    scenario::{BoardScenario, StatusScenario},
    seed::{Purpose, Seed, text_key},
};

/// The Next Train API's CDN serves a board for 10 seconds.
const BOARD_LIFETIME: i64 = 10;

/// The shortest freshness the real cache gives a board.
const BOARD_MIN_FRESH: i64 = 2;

/// The cause behind a simulated outage, which, like a real one, never
/// reaches API clients.
#[derive(Debug, Error)]
#[error("the mock scenario simulates an unreachable MTR")]
pub(crate) struct SimulatedOutage;

/// Next Train boards in one scenario and world.
///
/// Requests are validated, and a station's boards assembled, by the same
/// `NextTrainService` that serves real boards, so a mock response fails
/// exactly where a real one would.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimulatedNextTrains {
    scenario: BoardScenario,
    seed: Seed,
}

impl SimulatedNextTrains {
    pub const fn new(scenario: BoardScenario, seed: Seed) -> Self {
        Self { scenario, seed }
    }

    /// The board for `station` on `line`. An incident in the scenario, such
    /// as a delay, affects `line`.
    pub async fn board(
        &self,
        line: Line,
        station: StationCode,
    ) -> Result<BoardView, NextTrainError> {
        self.service(Some(line)).board(line, station).await
    }

    /// The boards of every line serving `station`. An incident in the
    /// scenario affects one of them, chosen by the seed, as incidents
    /// rarely hit every line at once.
    pub async fn station_boards(
        &self,
        station: StationCode,
    ) -> Result<StationBoards, NextTrainError> {
        self.service(self.incident_line(station))
            .station_boards(station)
            .await
    }

    fn service(&self, affected: Option<Line>) -> NextTrainService<ScenarioBoards> {
        NextTrainService::new(ScenarioBoards {
            scenario: self.scenario,
            seed: self.seed,
            affected,
        })
    }

    fn incident_line(&self, station: StationCode) -> Option<Line> {
        let lines = Line::serving(station).count() as u64;
        let mut rng = self
            .seed
            .stream(Purpose::Incident, &[text_key(station.as_str())]);
        Line::serving(station).nth(rng.below(lines) as usize)
    }
}

/// The source behind [`SimulatedNextTrains`]: every board it is asked for,
/// simulated as of the moment it is asked.
#[derive(Debug)]
struct ScenarioBoards {
    scenario: BoardScenario,
    seed: Seed,
    affected: Option<Line>,
}

impl NextTrainSource for ScenarioBoards {
    async fn board(
        &self,
        line: Line,
        station: StationCode,
    ) -> Result<Snapshot<NextTrainBoard>, SourceUnavailable> {
        let affected = self.affected == Some(line);
        let unavailable = match self.scenario {
            BoardScenario::UpstreamUnavailable => true,
            BoardScenario::PartialOutage => affected,
            _ => false,
        };
        if unavailable {
            return Err(SourceUnavailable::new(SimulatedOutage));
        }

        let fetch = Fetch::at(Timestamp::now(), self, line, station)?;
        let conditions = Conditions::of(self.scenario, affected);
        let board = board::simulate(line, station, conditions, self.seed, fetch.generated_at);
        Ok(Snapshot::new(
            Arc::new(board),
            fetch.fetched_at,
            fetch.freshness,
        ))
    }
}

/// When a board was fetched and generated, and how fresh it still is.
struct Fetch {
    fetched_at: Timestamp,
    generated_at: Timestamp,
    freshness: Freshness,
}

impl Fetch {
    /// A board is fetched afresh every 10 seconds, a few seconds after the
    /// MTR generated it. A stale board was fetched 40 to 80 seconds before
    /// the latest refresh, so it stays inside the 90 seconds the real cache
    /// serves one for.
    fn at(
        now: Timestamp,
        boards: &ScenarioBoards,
        line: Line,
        station: StationCode,
    ) -> Result<Self, SourceUnavailable> {
        let now = now.as_second();
        let refresh = now.div_euclid(BOARD_LIFETIME);
        let mut rng = boards.seed.stream(
            Purpose::Fetch,
            &[
                text_key(line.code()),
                text_key(station.as_str()),
                refresh as u64,
            ],
        );
        let (fetched_at, freshness) = if boards.scenario == BoardScenario::Stale {
            let refreshes_ago = rng.between(4, 8);
            ((refresh - refreshes_ago) * BOARD_LIFETIME, Freshness::Stale)
        } else {
            let fetched_at = refresh * BOARD_LIFETIME;
            let fresh_for = (BOARD_LIFETIME - (now - fetched_at)).max(BOARD_MIN_FRESH);
            let expires_in = Duration::from_secs(fresh_for as u64);
            (fetched_at, Freshness::Fresh { expires_in })
        };
        let generated_at = fetched_at - rng.between(2, 8);

        Ok(Self {
            fetched_at: timestamp(fetched_at)?,
            generated_at: timestamp(generated_at)?,
            freshness,
        })
    }
}

fn timestamp(second: i64) -> Result<Timestamp, SourceUnavailable> {
    Timestamp::from_second(second).map_err(SourceUnavailable::new)
}

/// The line status in one scenario and world.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimulatedLineStatus {
    scenario: StatusScenario,
    seed: Seed,
}

impl SimulatedLineStatus {
    pub const fn new(scenario: StatusScenario, seed: Seed) -> Self {
        Self { scenario, seed }
    }

    /// The status of every line as the polled feed would hold it now.
    pub fn status(&self) -> Result<Snapshot<NetworkStatus>, SourceUnavailable> {
        network_status::simulate(self.scenario, self.seed, Timestamp::now())
    }
}

#[cfg(test)]
mod tests {
    use dut_core::domain::network::Direction;
    use dut_core::station;

    use super::*;

    fn next_trains(scenario: BoardScenario) -> SimulatedNextTrains {
        SimulatedNextTrains::new(scenario, Seed::DEFAULT)
    }

    #[tokio::test]
    async fn a_board_is_fresh_for_at_most_ten_seconds() {
        let view = next_trains(BoardScenario::OffPeak)
            .board(Line::TseungKwanO, station!("TKO"))
            .await
            .expect("board should load");

        let Freshness::Fresh { expires_in } = view.snapshot().freshness() else {
            panic!("the board should be fresh");
        };
        assert!((2..=10).contains(&expires_in.as_secs()));
        let lag = view.snapshot().fetched_at().as_second() - view.board().generated_at.as_second();
        assert!((2..=8).contains(&lag));
        assert_eq!(view.directions().collect::<Vec<_>>(), Direction::ALL);
    }

    #[tokio::test]
    async fn requests_are_validated_as_for_real_boards() {
        let simulated = next_trains(BoardScenario::Peak);

        let off_line = simulated.board(Line::TseungKwanO, station!("ADM")).await;
        let unknown = simulated.station_boards(station!("XYZ")).await;

        assert!(matches!(
            off_line,
            Err(NextTrainError::StationNotOnLine { .. })
        ));
        assert!(matches!(unknown, Err(NextTrainError::UnknownStation(_))));
    }

    #[tokio::test]
    async fn a_partial_outage_fails_one_line_at_an_interchange() {
        let boards = next_trains(BoardScenario::PartialOutage)
            .station_boards(station!("ADM"))
            .await
            .expect("the other lines should load");

        let failed = boards
            .lines()
            .iter()
            .filter(|entry| entry.board.is_err())
            .count();
        assert_eq!(failed, 1);
        assert_eq!(boards.lines().len(), 4);
    }

    #[tokio::test]
    async fn a_partial_outage_fails_the_requested_line() {
        let result = next_trains(BoardScenario::PartialOutage)
            .board(Line::Island, station!("ADM"))
            .await;

        assert!(matches!(result, Err(NextTrainError::Unavailable(_))));
    }

    #[tokio::test]
    async fn an_unavailable_upstream_fails_every_line() {
        let result = next_trains(BoardScenario::UpstreamUnavailable)
            .station_boards(station!("ADM"))
            .await;

        assert!(matches!(result, Err(NextTrainError::Unavailable(_))));
    }

    #[tokio::test]
    async fn an_incident_at_a_station_affects_one_line() {
        let boards = next_trains(BoardScenario::Delayed)
            .station_boards(station!("ADM"))
            .await
            .expect("boards should load");

        let delayed = boards
            .lines()
            .iter()
            .filter(|entry| entry.board.as_ref().is_ok_and(|view| view.board().delayed))
            .count();
        assert_eq!(delayed, 1);
    }

    #[tokio::test]
    async fn stale_boards_are_about_a_minute_old() {
        let view = next_trains(BoardScenario::Stale)
            .board(Line::KwunTong, station!("KOT"))
            .await
            .expect("stale boards are still served");

        assert!(view.snapshot().freshness().is_stale());
        let age = Timestamp::now().as_second() - view.snapshot().fetched_at().as_second();
        assert!((40..=90).contains(&age), "{age}");
    }

    #[test]
    fn line_status_follows_its_scenario() {
        let typhoon = SimulatedLineStatus::new(StatusScenario::TyphoonSignal, Seed::DEFAULT)
            .status()
            .expect("typhoon status");

        assert_eq!(typhoon.value().lines.len(), 11);
        assert!(
            SimulatedLineStatus::new(StatusScenario::UpstreamUnavailable, Seed::DEFAULT)
                .status()
                .is_err()
        );
    }
}
