//! The platforms trains use, as the live Next Train API reported them on the
//! morning of 2026-10-02. The peak capture is kept in
//! `tests/fixtures/mtr/next_train_network.json`.
//!
//! Most stations take up trains on platform 1 and down trains on platform 2,
//! so only the others are listed. Platform numbers repeat between lines at
//! one station: Mei Foo has a platform 1 for each of its two lines.

use dut_core::domain::{
    network::{ByDirection, Direction, Line, StationCode},
    next_train::Platforms,
};

/// The platforms of trains in `direction` at `station` on `line`. Where
/// there are several, as at many termini, trains take them in turn.
pub(crate) fn at(line: Line, station: StationCode, direction: Direction) -> &'static [Platforms] {
    PLATFORMS
        .iter()
        .find(|entry| entry.line == line && entry.station == station)
        .map_or(USUAL.get(direction), |entry| entry.platforms.get(direction))
}

const USUAL: ByDirection<&[Platforms]> = ByDirection::new(&[one(1)], &[one(2)]);

struct StationPlatforms {
    line: Line,
    station: StationCode,
    platforms: ByDirection<&'static [Platforms]>,
}

/// An entry, checked at compile time.
const fn platforms(
    line: Line,
    station: &str,
    up: &'static [Platforms],
    down: &'static [Platforms],
) -> StationPlatforms {
    StationPlatforms {
        line,
        station: StationCode::from_static(station),
        platforms: ByDirection::new(up, down),
    }
}

/// One platform, spelt short for the table.
const fn one(number: u8) -> Platforms {
    Platforms::one(number)
}

/// A terminus lists only the direction that leaves it.
const PLATFORMS: &[StationPlatforms] = &[
    // Trains open their doors on both sides at Airport.
    platforms(
        Line::AirportExpress,
        "AIR",
        &[Platforms::pair(1, 3)],
        &[Platforms::pair(2, 4)],
    ),
    platforms(Line::AirportExpress, "AWE", &[one(1)], &[one(1)]),
    platforms(Line::TungChung, "HOK", &[one(3), one(4)], &[one(4)]),
    platforms(Line::TungChung, "KOW", &[one(3)], &[one(4)]),
    platforms(Line::TungChung, "NAC", &[one(3)], &[one(4)]),
    platforms(Line::TungChung, "LAK", &[one(3)], &[one(4)]),
    platforms(Line::TungChung, "TSY", &[one(3)], &[one(4)]),
    platforms(Line::TungChung, "TUC", &[one(1)], &[one(2), one(1)]),
    platforms(Line::TuenMa, "WKS", &[one(1), one(2)], &[one(2)]),
    platforms(Line::TuenMa, "TAW", &[one(3)], &[one(4)]),
    platforms(Line::TuenMa, "DIH", &[one(3)], &[one(4)]),
    platforms(Line::TuenMa, "HOM", &[one(3)], &[one(4)]),
    platforms(Line::TuenMa, "HUH", &[one(3)], &[one(2)]),
    platforms(Line::TuenMa, "TUM", &[one(1)], &[one(2), one(1)]),
    platforms(Line::TseungKwanO, "NOP", &[one(3)], &[one(3)]),
    platforms(Line::TseungKwanO, "QUB", &[one(3)], &[one(4)]),
    platforms(Line::TseungKwanO, "YAT", &[one(3)], &[one(4)]),
    platforms(Line::TseungKwanO, "TIK", &[one(3)], &[one(4)]),
    platforms(Line::TseungKwanO, "LHP", &[one(1)], &[one(2), one(1)]),
    platforms(Line::TseungKwanO, "POA", &[one(1)], &[one(1)]),
    platforms(Line::EastRail, "ADM", &[one(7)], &[one(7)]),
    platforms(Line::EastRail, "HUH", &[one(1)], &[one(4)]),
    platforms(Line::EastRail, "MKK", &[one(2)], &[one(3)]),
    platforms(Line::EastRail, "SHT", &[one(2)], &[one(3)]),
    platforms(Line::EastRail, "FOT", &[one(1)], &[one(4)]),
    platforms(Line::EastRail, "TAP", &[one(1)], &[one(4)]),
    platforms(Line::EastRail, "LOW", &[one(1)], &[one(4), one(1)]),
    platforms(Line::EastRail, "LMC", &[one(1)], &[one(1), one(2)]),
    // Seen live on 2026-09-28; the capture had only platform 5.
    platforms(Line::SouthIsland, "ADM", &[one(5), one(6)], &[one(5)]),
    platforms(Line::SouthIsland, "SOH", &[one(1)], &[one(1), one(2)]),
    platforms(Line::TsuenWan, "CEN", &[one(1), one(2)], &[one(2)]),
    platforms(Line::TsuenWan, "ADM", &[one(1)], &[one(4)]),
    platforms(Line::TsuenWan, "PRE", &[one(1)], &[one(4)]),
    platforms(Line::Island, "CEN", &[one(3)], &[one(4)]),
    platforms(Line::Island, "ADM", &[one(3)], &[one(2)]),
    platforms(Line::Island, "CHW", &[one(1)], &[one(1), one(2)]),
    platforms(Line::KwunTong, "YMT", &[one(3)], &[one(4)]),
    platforms(Line::KwunTong, "MOK", &[one(3)], &[one(4)]),
    platforms(Line::KwunTong, "PRE", &[one(3)], &[one(2)]),
    platforms(Line::KwunTong, "CHH", &[one(1)], &[one(4)]),
    platforms(Line::DisneylandResort, "SUN", &[one(3)], &[one(3)]),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::Value;

    use super::*;

    /// Every board of the 2026-10-02 capture, keyed `LINE-STA`.
    fn capture() -> Value {
        let text = include_str!("../../../tests/fixtures/mtr/next_train_network.json");
        serde_json::from_str(text).expect("capture should be valid JSON")
    }

    /// The platforms of each train in one direction of one captured board.
    fn captured(
        capture: &Value,
        line: Line,
        station: StationCode,
        direction: &str,
    ) -> Vec<Platforms> {
        let key = format!("{line}-{station}");
        capture[&key]["data"][&key][direction]
            .as_array()
            .map(|trains| {
                trains
                    .iter()
                    .filter_map(|train| train["plat"].as_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn every_captured_platform_is_listed() {
        let capture = capture();
        for line in Line::with_next_train() {
            for &station in line.stations() {
                for (direction, name) in [(Direction::Up, "UP"), (Direction::Down, "DOWN")] {
                    let listed = at(line, station, direction);
                    for platforms in captured(&capture, line, station, name) {
                        assert!(
                            listed.contains(&platforms),
                            "{line} {station} {direction:?} uses {platforms:?}, not {listed:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn entries_are_unique_and_on_their_line() {
        let mut seen = HashSet::new();
        for entry in PLATFORMS {
            assert!(entry.line.serves(entry.station), "{}", entry.station);
            assert!(
                seen.insert((entry.line, entry.station)),
                "{}",
                entry.station
            );
            assert!(!entry.platforms.up.is_empty() && !entry.platforms.down.is_empty());
        }
    }

    #[test]
    fn unlisted_stations_use_platforms_one_and_two() {
        let mong_kok = StationCode::from_static("MOK");

        assert_eq!(at(Line::TsuenWan, mong_kok, Direction::Up), [one(1)]);
        assert_eq!(at(Line::TsuenWan, mong_kok, Direction::Down), [one(2)]);
        assert_eq!(at(Line::KwunTong, mong_kok, Direction::Up), [one(3)]);
    }
}
