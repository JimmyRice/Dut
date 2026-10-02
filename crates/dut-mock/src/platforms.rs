//! The platforms trains use, as the live Next Train API reported them on the
//! morning of 2026-10-02. The peak capture is kept in
//! `tests/fixtures/mtr/next_train_network.json`.
//!
//! Most stations take up trains on platform 1 and down trains on platform 2,
//! so only the others are listed. Platform numbers repeat between lines at
//! one station: Mei Foo has a platform 1 for each of its two lines.

use dut_core::domain::network::{ByDirection, Direction, Line, StationCode};

/// The platforms trains in `direction` use at `station` on `line`. Where
/// there are several, as at many termini, trains take them in turn.
pub(crate) fn at(line: Line, station: StationCode, direction: Direction) -> &'static [u8] {
    PLATFORMS
        .iter()
        .find(|entry| entry.line == line && entry.station == station)
        .map_or(USUAL.get(direction), |entry| entry.platforms.get(direction))
}

const USUAL: ByDirection<&[u8]> = ByDirection::new(&[1], &[2]);

struct StationPlatforms {
    line: Line,
    station: StationCode,
    platforms: ByDirection<&'static [u8]>,
}

/// An entry, checked at compile time.
const fn platforms(
    line: Line,
    station: &str,
    up: &'static [u8],
    down: &'static [u8],
) -> StationPlatforms {
    StationPlatforms {
        line,
        station: StationCode::from_static(station),
        platforms: ByDirection::new(up, down),
    }
}

/// A terminus lists only the direction that leaves it.
const PLATFORMS: &[StationPlatforms] = &[
    // The MTR publishes "1/3" and "2/4" at Airport; this keeps the first.
    platforms(Line::AirportExpress, "AIR", &[1], &[2]),
    platforms(Line::AirportExpress, "AWE", &[1], &[1]),
    platforms(Line::TungChung, "HOK", &[3, 4], &[4]),
    platforms(Line::TungChung, "KOW", &[3], &[4]),
    platforms(Line::TungChung, "NAC", &[3], &[4]),
    platforms(Line::TungChung, "LAK", &[3], &[4]),
    platforms(Line::TungChung, "TSY", &[3], &[4]),
    platforms(Line::TungChung, "TUC", &[1], &[2, 1]),
    platforms(Line::TuenMa, "WKS", &[1, 2], &[2]),
    platforms(Line::TuenMa, "TAW", &[3], &[4]),
    platforms(Line::TuenMa, "DIH", &[3], &[4]),
    platforms(Line::TuenMa, "HOM", &[3], &[4]),
    platforms(Line::TuenMa, "HUH", &[3], &[2]),
    platforms(Line::TuenMa, "TUM", &[1], &[2, 1]),
    platforms(Line::TseungKwanO, "NOP", &[3], &[3]),
    platforms(Line::TseungKwanO, "QUB", &[3], &[4]),
    platforms(Line::TseungKwanO, "YAT", &[3], &[4]),
    platforms(Line::TseungKwanO, "TIK", &[3], &[4]),
    platforms(Line::TseungKwanO, "LHP", &[1], &[2, 1]),
    platforms(Line::TseungKwanO, "POA", &[1], &[1]),
    platforms(Line::EastRail, "ADM", &[7], &[7]),
    platforms(Line::EastRail, "HUH", &[1], &[4]),
    platforms(Line::EastRail, "MKK", &[2], &[3]),
    platforms(Line::EastRail, "SHT", &[2], &[3]),
    platforms(Line::EastRail, "FOT", &[1], &[4]),
    platforms(Line::EastRail, "TAP", &[1], &[4]),
    platforms(Line::EastRail, "LOW", &[1], &[4, 1]),
    platforms(Line::EastRail, "LMC", &[1], &[1, 2]),
    // Seen live on 2026-09-28; the capture had only platform 5.
    platforms(Line::SouthIsland, "ADM", &[5, 6], &[5]),
    platforms(Line::SouthIsland, "SOH", &[1], &[1, 2]),
    platforms(Line::TsuenWan, "CEN", &[1, 2], &[2]),
    platforms(Line::TsuenWan, "ADM", &[1], &[4]),
    platforms(Line::TsuenWan, "PRE", &[1], &[4]),
    platforms(Line::Island, "CEN", &[3], &[4]),
    platforms(Line::Island, "ADM", &[3], &[2]),
    platforms(Line::Island, "CHW", &[1], &[1, 2]),
    platforms(Line::KwunTong, "YMT", &[3], &[4]),
    platforms(Line::KwunTong, "MOK", &[3], &[4]),
    platforms(Line::KwunTong, "PRE", &[3], &[2]),
    platforms(Line::KwunTong, "CHH", &[1], &[4]),
    platforms(Line::DisneylandResort, "SUN", &[3], &[3]),
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

    /// The platforms in one direction of one captured board, reading "1/3"
    /// as 1.
    fn captured(capture: &Value, line: Line, station: StationCode, direction: &str) -> Vec<u8> {
        let key = format!("{line}-{station}");
        capture[&key]["data"][&key][direction]
            .as_array()
            .map(|trains| {
                trains
                    .iter()
                    .filter_map(|train| train["plat"].as_str()?.split('/').next()?.parse().ok())
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
                    for platform in captured(&capture, line, station, name) {
                        assert!(
                            listed.contains(&platform),
                            "{line} {station} {direction:?} uses platform {platform}, not {listed:?}"
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

        assert_eq!(at(Line::TsuenWan, mong_kok, Direction::Up), [1]);
        assert_eq!(at(Line::TsuenWan, mong_kok, Direction::Down), [2]);
        assert_eq!(at(Line::KwunTong, mong_kok, Direction::Up), [3]);
    }
}
