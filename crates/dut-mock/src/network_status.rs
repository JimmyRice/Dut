//! The simulated line status feed, as the poller would hold it.

use std::{sync::Arc, time::Duration};

use jiff::Timestamp;

use dut_core::{
    application::source::{Freshness, Snapshot, SourceUnavailable},
    domain::{
        line_status::{LineCondition, LineStatus, NetworkStatus},
        network::Line,
        time::HONG_KONG,
    },
};

use crate::{
    notices,
    scenario::StatusScenario,
    seed::{Purpose, Seed},
    simulated::SimulatedOutage,
};

/// The order the MTR's feed lists lines in.
const FEED_ORDER: [Line; 11] = [
    Line::TsuenWan,
    Line::KwunTong,
    Line::Island,
    Line::SouthIsland,
    Line::TseungKwanO,
    Line::TungChung,
    Line::DisneylandResort,
    Line::AirportExpress,
    Line::EastRail,
    Line::TuenMa,
    Line::LightRail,
];

/// The feed is polled every 30 seconds, and a poll stays fresh for its
/// interval plus the 3-second request timeout.
const POLL_INTERVAL: i64 = 30;
const FRESH_FOR: i64 = 33;

const HOUR: i64 = 60 * 60;
const DAY: i64 = 24 * HOUR;

/// What a status the MTR has not documented might look like.
const UNRECOGNISED_STATUS: &str = "blue";

/// The feed as polled at `now` in `scenario`.
pub(crate) fn simulate(
    scenario: StatusScenario,
    seed: Seed,
    now: Timestamp,
) -> Result<Snapshot<NetworkStatus>, SourceUnavailable> {
    if scenario == StatusScenario::UpstreamUnavailable {
        return Err(SourceUnavailable::new(SimulatedOutage));
    }
    let now = now.as_second();
    let mut incident = seed.stream(Purpose::Incident, &[]);
    let poll = now.div_euclid(POLL_INTERVAL);
    let (fetched_at, freshness) = if scenario == StatusScenario::Stale {
        // Between 2 and 12 minutes old, inside the 15 minutes a stale value
        // is served for.
        let polls_ago = incident.between(4, 24);
        ((poll - polls_ago) * POLL_INTERVAL, Freshness::Stale)
    } else {
        let fetched_at = poll * POLL_INTERVAL;
        let fresh_for = (FRESH_FOR - (now - fetched_at)) as u64;
        let expires_in = Duration::from_secs(fresh_for);
        (fetched_at, Freshness::Fresh { expires_in })
    };

    let affected = *incident.pick(&FEED_ORDER).unwrap_or(&Line::KwunTong);
    let lines = FEED_ORDER
        .into_iter()
        .map(|line| {
            let condition = condition(scenario, line == affected);
            let message = notices::incident(line, &condition, &mut incident);
            LineStatus {
                line,
                condition,
                message,
            }
        })
        .collect();
    let updated_at = updated_at(scenario, fetched_at, incident.between(0, HOUR - 1));

    let status = NetworkStatus {
        updated_at: timestamp(updated_at)?,
        lines,
    };
    Ok(Snapshot::new(
        Arc::new(status),
        timestamp(fetched_at)?,
        freshness,
    ))
}

fn condition(scenario: StatusScenario, affected: bool) -> LineCondition {
    match scenario {
        StatusScenario::TyphoonSignal => LineCondition::TyphoonSignal,
        StatusScenario::NonServiceHours => LineCondition::NonServiceHours,
        StatusScenario::Delayed if affected => LineCondition::Delayed,
        StatusScenario::Disrupted if affected => LineCondition::Disrupted,
        StatusScenario::DelayedOrDisrupted if affected => LineCondition::DelayedOrDisrupted,
        StatusScenario::UnknownCondition if affected => {
            LineCondition::Unknown(UNRECOGNISED_STATUS.to_owned())
        }
        StatusScenario::Normal
        | StatusScenario::Delayed
        | StatusScenario::Disrupted
        | StatusScenario::DelayedOrDisrupted
        | StatusScenario::UnknownCondition
        | StatusScenario::Stale
        | StatusScenario::UpstreamUnavailable => LineCondition::Normal,
    }
}

/// When the MTR last rebuilt the feed, which it does only when a line
/// changes: as service starts at 06:15, as the last line closes at 01:20,
/// or when an incident is reported, here within the hour before the poll.
fn updated_at(scenario: StatusScenario, fetched_at: i64, incident_offset: i64) -> i64 {
    match scenario {
        StatusScenario::Normal | StatusScenario::Stale | StatusScenario::UpstreamUnavailable => {
            latest_daily(fetched_at, 6, 15)
        }
        StatusScenario::NonServiceHours => latest_daily(fetched_at, 1, 20),
        StatusScenario::Delayed
        | StatusScenario::Disrupted
        | StatusScenario::DelayedOrDisrupted
        | StatusScenario::TyphoonSignal
        | StatusScenario::UnknownCondition => latest(fetched_at, HOUR, incident_offset),
    }
}

/// The latest `hour:minute` Hong Kong Time at or before `time`.
fn latest_daily(time: i64, hour: i64, minute: i64) -> i64 {
    let offset = i64::from(HONG_KONG.seconds());
    latest(time, DAY, hour * HOUR + minute * 60 - offset)
}

/// The latest time at or before `time` that is `phase` seconds into a
/// `period`.
fn latest(time: i64, period: i64, phase: i64) -> i64 {
    time - (time - phase).rem_euclid(period)
}

fn timestamp(second: i64) -> Result<Timestamp, SourceUnavailable> {
    Timestamp::from_second(second).map_err(SourceUnavailable::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-02T00:30:12Z".parse().expect("valid timestamp")
    }

    fn status(scenario: StatusScenario, seed: u64) -> Snapshot<NetworkStatus> {
        simulate(scenario, Seed::new(seed), now()).expect("the scenario has a status")
    }

    fn conditions(snapshot: &Snapshot<NetworkStatus>) -> Vec<&LineCondition> {
        snapshot
            .value()
            .lines
            .iter()
            .map(|line| &line.condition)
            .collect()
    }

    #[test]
    fn lists_every_line_in_the_feed_order() {
        let snapshot = status(StatusScenario::Normal, 0);
        let lines: Vec<Line> = snapshot
            .value()
            .lines
            .iter()
            .map(|line| line.line)
            .collect();

        assert_eq!(lines, FEED_ORDER);
        assert!(
            conditions(&snapshot)
                .iter()
                .all(|condition| **condition == LineCondition::Normal)
        );
        assert!(
            snapshot
                .value()
                .lines
                .iter()
                .all(|line| line.message.is_none())
        );
    }

    #[test]
    fn a_normal_feed_was_last_rebuilt_when_service_started() {
        let snapshot = status(StatusScenario::Normal, 0);

        assert_eq!(
            snapshot
                .value()
                .updated_at
                .display_with_offset(HONG_KONG)
                .to_string(),
            "2026-10-02T06:15:00+08:00"
        );
        assert_eq!(snapshot.fetched_at().to_string(), "2026-10-02T00:30:00Z");
        assert_eq!(
            snapshot.freshness(),
            Freshness::Fresh {
                expires_in: Duration::from_secs(21)
            }
        );
    }

    #[test]
    fn an_incident_affects_one_line_and_is_explained() {
        for (scenario, expected) in [
            (StatusScenario::Delayed, LineCondition::Delayed),
            (StatusScenario::Disrupted, LineCondition::Disrupted),
            (
                StatusScenario::DelayedOrDisrupted,
                LineCondition::DelayedOrDisrupted,
            ),
        ] {
            for seed in 0..20 {
                let snapshot = status(scenario, seed);
                let affected: Vec<&LineStatus> = snapshot
                    .value()
                    .lines
                    .iter()
                    .filter(|line| line.condition != LineCondition::Normal)
                    .collect();

                assert_eq!(affected.len(), 1, "{scenario:?} {seed}");
                assert_eq!(affected[0].condition, expected);
                assert!(affected[0].message.is_some(), "{scenario:?} {seed}");
                let updated_at = snapshot.value().updated_at;
                assert!(updated_at <= snapshot.fetched_at());
                assert!(snapshot.fetched_at().as_second() - updated_at.as_second() < HOUR);
            }
        }
    }

    #[test]
    fn the_seed_picks_the_affected_line() {
        let affected = |seed| {
            status(StatusScenario::Delayed, seed)
                .value()
                .lines
                .iter()
                .find(|line| line.condition == LineCondition::Delayed)
                .map(|line| line.line)
        };

        assert_eq!(affected(5), affected(5));
        let lines: std::collections::HashSet<_> = (0..50).filter_map(affected).collect();
        assert!(lines.len() > 5);
    }

    #[test]
    fn network_wide_conditions_cover_every_line() {
        let typhoon = status(StatusScenario::TyphoonSignal, 0);
        let night = status(StatusScenario::NonServiceHours, 0);

        assert!(
            conditions(&typhoon)
                .iter()
                .all(|condition| **condition == LineCondition::TyphoonSignal)
        );
        assert!(
            conditions(&night)
                .iter()
                .all(|condition| **condition == LineCondition::NonServiceHours)
        );
        assert_eq!(
            night
                .value()
                .updated_at
                .display_with_offset(HONG_KONG)
                .to_string(),
            "2026-10-02T01:20:00+08:00"
        );
    }

    #[test]
    fn an_unrecognised_status_is_kept_verbatim() {
        let snapshot = status(StatusScenario::UnknownCondition, 0);

        assert!(
            conditions(&snapshot)
                .contains(&&LineCondition::Unknown(UNRECOGNISED_STATUS.to_owned()))
        );
    }

    #[test]
    fn a_stale_feed_is_minutes_old() {
        let snapshot = status(StatusScenario::Stale, 0);
        let age = now().as_second() - snapshot.fetched_at().as_second();

        assert_eq!(snapshot.freshness(), Freshness::Stale);
        assert!((2 * 60..=12 * 60 + 30).contains(&age), "{age}");
    }

    #[test]
    fn an_unavailable_upstream_has_no_status() {
        assert!(simulate(StatusScenario::UpstreamUnavailable, Seed::DEFAULT, now()).is_err());
    }

    #[test]
    fn finds_the_latest_time_of_day() {
        // 2026-10-02T00:30:12Z is 08:30:12 in Hong Kong.
        let time = now().as_second();

        assert_eq!(time - latest_daily(time, 8, 30), 12);
        assert_eq!(time - latest_daily(time, 8, 31), DAY - 48);
        assert_eq!(latest(time, HOUR, 0) % HOUR, 0);
    }
}
