//! The situations the mock API simulates, and how a client picks one.

use std::{fmt, str::FromStr};

use dut_core::domain::localized::Localized;
use thiserror::Error;

use crate::seed::{Purpose, Seed};

/// What clients pass as `?scenario=` to have one drawn at random.
const RANDOM: &str = "random";

/// A situation the mock API simulates for one kind of data. Clients name it
/// in `?scenario=`, and the catalogue lists it with its description.
pub trait Scenario: Copy + fmt::Debug + Eq + Send + Sync + 'static {
    /// Every scenario, in the order the catalogue lists them.
    const ALL: &'static [Self];

    /// The everyday situation, which `random` falls back to.
    const USUAL: Self;

    /// The identifier clients pass in `?scenario=`.
    fn name(self) -> &'static str;

    /// What a client sees in this scenario, for a developer choosing one.
    fn description(self) -> Localized<&'static str>;

    /// How often `random` picks this scenario, relative to the others. Zero
    /// for one that is useful to name but unrealistic to stumble on.
    fn random_weight(self) -> u32;
}

/// What a client asked for: a named scenario, or one drawn at random.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScenarioChoice<S> {
    /// Drawn by weight from the seed, so a seed reproduces the draw too.
    #[default]
    Random,
    Named(S),
}

impl<S: Scenario> ScenarioChoice<S> {
    /// The scenario to simulate and the world to simulate it in.
    ///
    /// Without a seed from the client, a named scenario is simulated in
    /// [`Seed::DEFAULT`], so polling it shows the same trains moving on,
    /// whereas `random` draws a fresh seed, and with it a fresh scenario,
    /// for every request.
    pub fn resolve(self, seed: Option<Seed>) -> (S, Seed) {
        match self {
            Self::Named(scenario) => (scenario, seed.unwrap_or(Seed::DEFAULT)),
            Self::Random => {
                let seed = seed.unwrap_or_else(Seed::fresh);
                (draw(seed), seed)
            }
        }
    }
}

/// Parses `random` or a scenario's name, case-insensitively.
impl<S: Scenario> FromStr for ScenarioChoice<S> {
    type Err = UnknownScenario;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.eq_ignore_ascii_case(RANDOM) {
            return Ok(Self::Random);
        }
        S::ALL
            .iter()
            .copied()
            .find(|scenario| scenario.name().eq_ignore_ascii_case(input))
            .map(Self::Named)
            .ok_or(UnknownScenario)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("unknown mock scenario")]
pub struct UnknownScenario;

/// Picks a scenario by weight.
fn draw<S: Scenario>(seed: Seed) -> S {
    let total: u64 = S::ALL
        .iter()
        .map(|scenario| u64::from(scenario.random_weight()))
        .sum();
    let mut ticket = seed.stream(Purpose::Scenario, &[]).below(total);
    S::ALL
        .iter()
        .copied()
        .find(|scenario| {
            let weight = u64::from(scenario.random_weight());
            let drawn = ticket < weight;
            ticket = ticket.saturating_sub(weight);
            drawn
        })
        .unwrap_or(S::USUAL)
}

/// What the simulated Next Train API reports.
///
/// The parts of the day set how often trains run. An incident, such as a
/// delay or a notice, affects one line: the requested one, or at a
/// station's boards one of the lines serving it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoardScenario {
    Peak,
    OffPeak,
    LateNight,
    LastTrain,
    NonServiceHours,
    Delayed,
    SpecialArrangement,
    RaceDay,
    Stale,
    PartialOutage,
    UpstreamUnavailable,
}

impl Scenario for BoardScenario {
    const ALL: &'static [Self] = &[
        Self::Peak,
        Self::OffPeak,
        Self::LateNight,
        Self::LastTrain,
        Self::NonServiceHours,
        Self::Delayed,
        Self::SpecialArrangement,
        Self::RaceDay,
        Self::Stale,
        Self::PartialOutage,
        Self::UpstreamUnavailable,
    ];

    const USUAL: Self = Self::OffPeak;

    fn name(self) -> &'static str {
        match self {
            Self::Peak => "peak",
            Self::OffPeak => "off_peak",
            Self::LateNight => "late_night",
            Self::LastTrain => "last_train",
            Self::NonServiceHours => "non_service_hours",
            Self::Delayed => "delayed",
            Self::SpecialArrangement => "special_arrangement",
            Self::RaceDay => "race_day",
            Self::Stale => "stale",
            Self::PartialOutage => "partial_outage",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }

    fn description(self) -> Localized<&'static str> {
        match self {
            Self::Peak => Localized::new(
                "Rush hour: trains every two to three minutes on most lines, some of them turning back short of the terminus.",
                "繁忙時間：大部分綫路兩至三分鐘一班，部分班次在中途站折返。",
            ),
            Self::OffPeak => Localized::new(
                "Daytime outside rush hour: trains every three to five minutes on most lines.",
                "非繁忙時間：大部分綫路三至五分鐘一班。",
            ),
            Self::LateNight => Localized::new(
                "After 11 pm: trains every six to ten minutes, and LOHAS Park trains run only to Tiu Keng Leng.",
                "晚上十一時後：六至十分鐘一班，康城列車只行駛至調景嶺。",
            ),
            Self::LastTrain => Localized::new(
                "The last trains of the night: some directions have a train or two left, others none.",
                "尾班車：部分方向只剩一兩班列車，部分已經沒有列車。",
            ),
            Self::NonServiceHours => Localized::new(
                "Overnight, when no trains run: every direction is listed without trains.",
                "非服務時間：每個方向照常列出，但沒有列車。",
            ),
            Self::Delayed => Localized::new(
                "Rush hour with one line flagged as delayed: its trains run late, unevenly, and bunched.",
                "繁忙時間，其中一條綫標示為延誤：列車脫班、班距不均。",
            ),
            Self::SpecialArrangement => Localized::new(
                "One line carries a special train service arrangement notice; its trains are still listed.",
                "其中一條綫附有特別列車服務安排通告，列車照常列出。",
            ),
            Self::RaceDay => Localized::new(
                "Sha Tin race day: some East Rail Line trains call at Racecourse instead of Fo Tan.",
                "沙田賽馬日：部分東鐵綫列車停馬場站，不停火炭站。",
            ),
            Self::Stale => Localized::new(
                "The MTR cannot be reached, so boards from about a minute ago are served, marked stale.",
                "暫時無法連接港鐵，返回約一分鐘前的資料，並標示為過時。",
            ),
            Self::PartialOutage => Localized::new(
                "One line's board cannot be loaded; at an interchange the other lines still are.",
                "其中一條綫的資料無法取得；轉車站的其他綫路照常返回。",
            ),
            Self::UpstreamUnavailable => Localized::new(
                "The MTR cannot be reached and no recent data is left, so the request fails with 502.",
                "無法連接港鐵，亦沒有近期資料，請求返回 502。",
            ),
        }
    }

    fn random_weight(self) -> u32 {
        match self {
            Self::OffPeak => 4,
            Self::Peak => 3,
            Self::LateNight | Self::Delayed => 2,
            Self::LastTrain
            | Self::NonServiceHours
            | Self::SpecialArrangement
            | Self::RaceDay
            | Self::Stale
            | Self::PartialOutage
            | Self::UpstreamUnavailable => 1,
        }
    }
}

/// What the simulated line status feed reports. An incident affects one
/// line, which may be Light Rail.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StatusScenario {
    Normal,
    Delayed,
    Disrupted,
    DelayedOrDisrupted,
    TyphoonSignal,
    NonServiceHours,
    UnknownCondition,
    Stale,
    UpstreamUnavailable,
}

impl Scenario for StatusScenario {
    const ALL: &'static [Self] = &[
        Self::Normal,
        Self::Delayed,
        Self::Disrupted,
        Self::DelayedOrDisrupted,
        Self::TyphoonSignal,
        Self::NonServiceHours,
        Self::UnknownCondition,
        Self::Stale,
        Self::UpstreamUnavailable,
    ];

    const USUAL: Self = Self::Normal;

    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Delayed => "delayed",
            Self::Disrupted => "disrupted",
            Self::DelayedOrDisrupted => "delayed_or_disrupted",
            Self::TyphoonSignal => "typhoon_signal",
            Self::NonServiceHours => "non_service_hours",
            Self::UnknownCondition => "unknown_condition",
            Self::Stale => "stale",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }

    fn description(self) -> Localized<&'static str> {
        match self {
            Self::Normal => Localized::new("Good service on every line.", "所有綫路服務正常。"),
            Self::Delayed => Localized::new(
                "One line is delayed, with the MTR's explanation.",
                "其中一條綫延誤，附港鐵的說明。",
            ),
            Self::Disrupted => Localized::new(
                "Service on part of one line is suspended, with the MTR's explanation.",
                "其中一條綫部分路段暫停服務，附港鐵的說明。",
            ),
            Self::DelayedOrDisrupted => Localized::new(
                "One line is delayed or disrupted, which the MTR website shows in yellow.",
                "其中一條綫延誤或受阻，港鐵網站以黃色顯示。",
            ),
            Self::TyphoonSignal => Localized::new(
                "A tropical cyclone warning signal is in force on every line.",
                "所有綫路正受熱帶氣旋警告信號影響。",
            ),
            Self::NonServiceHours => Localized::new(
                "Overnight: every line is outside service hours.",
                "深夜：所有綫路處於非服務時間。",
            ),
            Self::UnknownCondition => Localized::new(
                "One line reports a status this service does not recognise yet. Never drawn at random.",
                "其中一條綫的狀態本服務尚未能識別。隨機場景不會抽中。",
            ),
            Self::Stale => Localized::new(
                "The MTR cannot be reached, so the status from a few minutes ago is served, marked stale.",
                "暫時無法連接港鐵，返回數分鐘前的狀態，並標示為過時。",
            ),
            Self::UpstreamUnavailable => Localized::new(
                "The MTR cannot be reached and no recent status is left, so the request fails with 502.",
                "無法連接港鐵，亦沒有近期狀態，請求返回 502。",
            ),
        }
    }

    fn random_weight(self) -> u32 {
        match self {
            Self::Normal => 8,
            Self::Delayed => 3,
            Self::NonServiceHours => 2,
            Self::Disrupted
            | Self::DelayedOrDisrupted
            | Self::TyphoonSignal
            | Self::Stale
            | Self::UpstreamUnavailable => 1,
            Self::UnknownCondition => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn names<S: Scenario>() -> Vec<&'static str> {
        S::ALL.iter().map(|scenario| scenario.name()).collect()
    }

    #[test]
    fn names_are_unique_and_parse_back() {
        fn check<S: Scenario>() {
            let names = names::<S>();
            let unique: HashSet<_> = names.iter().collect();
            assert_eq!(unique.len(), names.len());
            assert!(!names.contains(&RANDOM));
            for &scenario in S::ALL {
                assert_eq!(
                    scenario.name().to_uppercase().parse(),
                    Ok(ScenarioChoice::Named(scenario))
                );
            }
        }
        check::<BoardScenario>();
        check::<StatusScenario>();
    }

    #[test]
    fn parses_random_and_rejects_anything_else() {
        assert_eq!(
            "Random".parse(),
            Ok(ScenarioChoice::<BoardScenario>::Random)
        );
        assert_eq!(
            "typhoon".parse::<ScenarioChoice<StatusScenario>>(),
            Err(UnknownScenario)
        );
        assert_eq!(
            "".parse::<ScenarioChoice<StatusScenario>>(),
            Err(UnknownScenario)
        );
    }

    #[test]
    fn a_named_scenario_defaults_to_the_shared_world() {
        let choice = ScenarioChoice::Named(BoardScenario::Peak);

        assert_eq!(choice.resolve(None), (BoardScenario::Peak, Seed::DEFAULT));
        assert_eq!(
            choice.resolve(Some(Seed::new(9))),
            (BoardScenario::Peak, Seed::new(9))
        );
    }

    #[test]
    fn a_random_draw_is_reproduced_by_its_seed() {
        let choice = ScenarioChoice::<StatusScenario>::Random;
        let (scenario, seed) = choice.resolve(None);

        assert_eq!(choice.resolve(Some(seed)), (scenario, seed));
    }

    #[test]
    fn random_draws_follow_the_weights() {
        fn drawn<S: Scenario + std::hash::Hash>() -> HashSet<S> {
            (0..2_000).map(|seed| draw::<S>(Seed::new(seed))).collect()
        }

        let boards = drawn::<BoardScenario>();
        assert_eq!(boards.len(), BoardScenario::ALL.len());

        let statuses = drawn::<StatusScenario>();
        assert!(!statuses.contains(&StatusScenario::UnknownCondition));
        assert_eq!(statuses.len(), StatusScenario::ALL.len() - 1);
    }

    #[test]
    fn every_scenario_is_described_in_both_languages() {
        fn check<S: Scenario>() {
            for &scenario in S::ALL {
                let description = scenario.description();
                assert!(description.en.ends_with('.'), "{scenario:?}");
                assert!(description.tc.ends_with('。'), "{scenario:?}");
            }
        }
        check::<BoardScenario>();
        check::<StatusScenario>();
    }
}
