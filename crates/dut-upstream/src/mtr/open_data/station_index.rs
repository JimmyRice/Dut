use std::collections::BTreeMap;

use tracing::debug;

use dut_core::domain::network::{Station, StationCode};

/// Resolves the numeric station IDs that fare and facility files use to
/// station codes.
///
/// IDs come from the station list, where Hong Kong, Kowloon, and Tsing Yi
/// each have two: one for the Tung Chung Line and one for the Airport
/// Express. The station list leaves Racecourse out, so its ID is learned
/// from the fare file by matching the station's English name against the
/// compiled network.
#[derive(Debug, Default)]
pub(super) struct StationIndex {
    codes: BTreeMap<u16, StationCode>,
}

impl StationIndex {
    pub(super) fn insert(&mut self, id: u16, code: StationCode) {
        self.codes.entry(id).or_insert(code);
    }

    pub(super) fn get(&self, id: u16) -> Option<StationCode> {
        self.codes.get(&id).copied()
    }

    /// Resolves `id` by the station's English name when the station list
    /// did not, and remembers the answer.
    pub(super) fn learn(&mut self, id: u16, english_name: &str) -> Option<StationCode> {
        if let Some(code) = self.get(id) {
            return Some(code);
        }
        let station = Station::all()
            .iter()
            .find(|station| station.name.en.eq_ignore_ascii_case(english_name))?;
        debug!(id, station = %station.code, "resolved a station ID by its name");
        self.codes.insert(id, station.code);
        Some(station.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(code: &str) -> StationCode {
        code.parse().expect("test station code should be valid")
    }

    #[test]
    fn keeps_the_first_code_for_an_id() {
        let mut index = StationIndex::default();
        index.insert(39, code("HOK"));
        index.insert(44, code("HOK"));
        index.insert(39, code("KOW"));

        assert_eq!(index.get(39), Some(code("HOK")));
        assert_eq!(index.get(44), Some(code("HOK")));
        assert_eq!(index.get(40), None);
    }

    #[test]
    fn learns_unlisted_ids_by_name() {
        let mut index = StationIndex::default();

        assert_eq!(index.learn(70, "Racecourse"), Some(code("RAC")));
        assert_eq!(index.get(70), Some(code("RAC")));
        assert_eq!(index.learn(888, "Nowhere"), None);
    }

    #[test]
    fn prefers_the_station_list_over_names() {
        let mut index = StationIndex::default();
        index.insert(1, code("CEN"));

        assert_eq!(index.learn(1, "Admiralty"), Some(code("CEN")));
    }
}
