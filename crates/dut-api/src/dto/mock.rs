use serde::{Deserialize, Serialize};

use dut_mock::{BoardScenario, Scenario, StatusScenario};

use crate::dto::common::LocalizedText;

/// The query string every mock route accepts: `?scenario=…&seed=…`.
#[derive(Debug, Deserialize)]
pub(crate) struct MockQuery {
    /// A scenario's name, or `random`; `random` when absent.
    pub scenario: Option<String>,
    pub seed: Option<u64>,
}

/// `GET /api/mock/scenarios`: what each mock route can simulate.
#[derive(Debug, Serialize)]
pub(crate) struct ScenariosResponse {
    pub next_trains: Vec<ScenarioEntry>,
    pub line_status: Vec<ScenarioEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ScenarioEntry {
    /// What a client passes as `?scenario=`.
    pub scenario: &'static str,
    /// How often `random` picks it, relative to the other entries.
    pub random_weight: u32,
    pub description: LocalizedText<'static>,
}

impl ScenariosResponse {
    pub(crate) fn catalogue() -> Self {
        Self {
            next_trains: entries::<BoardScenario>(),
            line_status: entries::<StatusScenario>(),
        }
    }
}

fn entries<S: Scenario>() -> Vec<ScenarioEntry> {
    S::ALL
        .iter()
        .map(|&scenario| ScenarioEntry {
            scenario: scenario.name(),
            random_weight: scenario.random_weight(),
            description: scenario.description().into(),
        })
        .collect()
}
