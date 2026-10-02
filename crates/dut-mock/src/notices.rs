//! What the MTR says about special arrangements and incidents, worded as
//! its notices are.

use dut_core::domain::{
    line_status::LineCondition,
    localized::Localized,
    network::{Direction, Line, Station, StationCode},
    next_train::AlertNotice,
};

use crate::{seed::Rng, timetable::hops};

/// Where the Next Train API's notices link to: the MTR's traffic news for
/// phones, in the language the phone uses.
const TRAFFIC_NEWS: &str = "https://www.mtr.com.hk/alert/alert_title_wap.html";

/// The notice the Next Train API publishes for a line under special
/// arrangements, in both languages.
pub(crate) fn special_arrangement() -> Localized<AlertNotice> {
    let notice = |message: &str| AlertNotice {
        message: message.to_owned(),
        url: Some(TRAFFIC_NEWS.to_owned()),
    };
    Localized::new(
        notice(
            "Special train service arrangements are now in place on this line. Please click here for more information.",
        ),
        notice("此綫路現正實施特別列車服務安排，詳情請按此。"),
    )
}

/// What the line status feed says about `condition` on `line`, or `None`
/// for a condition the MTR does not explain.
pub(crate) fn incident(line: Line, condition: &LineCondition, rng: &mut Rng) -> Option<String> {
    if !matches!(
        condition,
        LineCondition::Delayed | LineCondition::Disrupted | LineCondition::DelayedOrDisrupted
    ) {
        return None;
    }
    if line == Line::LightRail {
        return light_rail_incident(condition, rng);
    }
    let name = line.name().en;
    let station = station_name(*rng.pick(line.stations())?)?;
    let message = match condition {
        LineCondition::Delayed => match rng.below(4) {
            0 => format!(
                "Due to a signalling fault at {station} Station, {name} train service is delayed. Passengers please allow extra travelling time."
            ),
            1 => format!(
                "Due to a train fault at {station} Station, {name} trains are running at a lower frequency. Passengers please allow extra travelling time."
            ),
            2 => format!(
                "Due to a passenger incident at {station} Station, {name} train service is delayed. Passengers please allow extra travelling time."
            ),
            _ => format!(
                "{name} train service is gradually resuming. Passengers please allow extra travelling time."
            ),
        },
        LineCondition::Disrupted => {
            let (from, to) = section(line, rng)?;
            if rng.percent(50) {
                format!(
                    "Due to a signalling fault, {name} train service between {from} and {to} stations is suspended. Passengers are advised to use other means of transport."
                )
            } else {
                format!(
                    "Due to a power supply fault, {name} train service between {from} and {to} stations is suspended. Free shuttle buses are being arranged. Passengers are advised to use other means of transport."
                )
            }
        }
        LineCondition::DelayedOrDisrupted => format!(
            "Due to a track fault near {station} Station, {name} train service is affected. Passengers please allow extra travelling time or use other means of transport."
        ),
        _ => return None,
    };
    Some(message)
}

/// Light Rail runs on streets, so its incidents name routes, not stations.
fn light_rail_incident(condition: &LineCondition, rng: &mut Rng) -> Option<String> {
    const ROUTE_PAIRS: [(&str, &str); 4] = [
        ("505", "610"),
        ("614", "615"),
        ("705", "706"),
        ("751", "761P"),
    ];
    let (first, second) = *rng.pick(&ROUTE_PAIRS)?;
    let message = match condition {
        LineCondition::Delayed => format!(
            "Due to a traffic accident, Light Rail routes {first} and {second} are delayed. Passengers please allow extra travelling time."
        ),
        LineCondition::Disrupted => format!(
            "Due to a road traffic incident, Light Rail routes {first} and {second} are suspended. Free shuttle buses are being arranged."
        ),
        LineCondition::DelayedOrDisrupted => format!(
            "Due to a vehicle fault, Light Rail routes {first} and {second} are affected. Passengers please allow extra travelling time."
        ),
        _ => return None,
    };
    Some(message)
}

/// Two stations a few stops apart on one branch of `line`, as the ends of a
/// suspended section.
fn section(line: Line, rng: &mut Rng) -> Option<(&'static str, &'static str)> {
    let stations = line.stations();
    let from = *rng.pick(stations)?;
    let direction = if line.towards(from, Direction::Up).next().is_some() {
        Direction::Up
    } else {
        Direction::Down
    };
    let ends: Vec<StationCode> = stations
        .iter()
        .copied()
        .filter(|&to| matches!(hops(line, from, to, direction), Some(1..=3)))
        .collect();
    let to = *rng.pick(&ends)?;
    Some((station_name(from)?, station_name(to)?))
}

fn station_name(code: StationCode) -> Option<&'static str> {
    Station::find(code).map(|station| station.name.en)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::{Purpose, Seed};

    const INCIDENTS: [LineCondition; 3] = [
        LineCondition::Delayed,
        LineCondition::Disrupted,
        LineCondition::DelayedOrDisrupted,
    ];

    #[test]
    fn every_line_explains_every_incident() {
        for line in Line::ALL {
            for condition in &INCIDENTS {
                for seed in 0..20 {
                    let mut rng = Seed::new(seed).stream(Purpose::Incident, &[]);
                    let message = incident(line, condition, &mut rng)
                        .unwrap_or_else(|| panic!("{line} {condition:?} has no message"));
                    assert!(message.ends_with('.'), "{message}");
                    assert!(!message.contains('{'), "{message}");
                }
            }
        }
    }

    #[test]
    fn a_suspension_names_the_line_and_two_of_its_stations() {
        let mut rng = Seed::new(3).stream(Purpose::Incident, &[]);

        let message = incident(Line::KwunTong, &LineCondition::Disrupted, &mut rng)
            .expect("a suspension is explained");

        let (from, to) = message
            .split_once("Kwun Tong Line train service between ")
            .and_then(|(_, rest)| rest.split_once(" stations is suspended"))
            .and_then(|(section, _)| section.split_once(" and "))
            .unwrap_or_else(|| panic!("no section in {message}"));
        let on_line = |name| {
            Line::KwunTong
                .stations()
                .iter()
                .any(|&code| station_name(code) == Some(name))
        };
        assert!(on_line(from) && on_line(to) && from != to, "{message}");
    }

    #[test]
    fn routine_conditions_carry_no_message() {
        let mut rng = Seed::DEFAULT.stream(Purpose::Incident, &[]);

        for condition in [
            LineCondition::Normal,
            LineCondition::NonServiceHours,
            LineCondition::TyphoonSignal,
        ] {
            assert_eq!(incident(Line::Island, &condition, &mut rng), None);
        }
    }

    #[test]
    fn the_special_arrangement_notice_links_to_traffic_news_in_both_languages() {
        let notice = special_arrangement();

        assert!(
            notice
                .en
                .message
                .starts_with("Special train service arrangements")
        );
        assert!(notice.tc.message.contains("特別列車服務安排"));
        assert_eq!(notice.en.url.as_deref(), Some(TRAFFIC_NEWS));
        assert_eq!(notice.tc.url, notice.en.url);
    }
}
