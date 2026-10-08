//! `barrier_free_facility_category.csv` (the catalogue) and
//! `barrier_free_facilities.csv` (which station provides what).

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use dut_core::domain::{
    network::StationCode,
    reference::{
        Accessibility, Facility, FacilityCategory, FacilityCode, FacilityGroup, SourceFile,
        StationAccessibility, StationFacility,
    },
};

use crate::mtr::open_data::{
    decode::{clean_localized, decode_rows, parse},
    error::OpenDataError,
    skipped::Skipped,
    station_index::StationIndex,
};

const CATALOGUE: SourceFile = SourceFile::BarrierFreeFacilityCategories;
const FACILITIES: SourceFile = SourceFile::BarrierFreeFacilities;

#[derive(Debug, Deserialize)]
struct CategoryRow {
    #[serde(rename = "Item_Code")]
    code: String,
    #[serde(rename = "Category_Id")]
    category: String,
    #[serde(rename = "Category_En")]
    category_en: String,
    #[serde(rename = "Category_Zh")]
    category_tc: String,
    #[serde(rename = "Facility_En")]
    name_en: String,
    #[serde(rename = "Facility_Zh")]
    name_tc: String,
    /// Within the category; some values repeat.
    #[serde(rename = "Sorting_Order")]
    order: u16,
}

/// Whether one station provides one facility. The file also has an
/// `Exit_Coordinate_X_Y` column, which is always empty.
#[derive(Debug, Deserialize)]
struct FacilityRow {
    #[serde(rename = "Station_No")]
    station_id: u16,
    #[serde(rename = "Key")]
    code: String,
    /// `Y` or `N`.
    #[serde(rename = "Value")]
    provided: String,
    #[serde(rename = "AJTextEn")]
    location_en: String,
    #[serde(rename = "AJTextZh")]
    location_tc: String,
}

/// The catalogue grouped by category, and the facilities each station
/// provides, in catalogue order. Stations that provide nothing listed,
/// such as the unidentified station `888`, are left out.
pub(super) fn clean(
    catalogue: &[u8],
    facilities: &[u8],
    index: &StationIndex,
) -> Result<Accessibility, OpenDataError> {
    let categories = clean_catalogue(catalogue)?;
    let rank: HashMap<&FacilityCode, usize> = categories
        .iter()
        .flat_map(|group| &group.facilities)
        .enumerate()
        .map(|(rank, facility)| (&facility.code, rank))
        .collect();

    let mut unknown_stations = Skipped::new(FACILITIES, "unknown station ID");
    let mut unknown_facilities = Skipped::new(FACILITIES, "facility missing from the catalogue");
    let mut stations: BTreeMap<StationCode, Vec<(usize, StationFacility)>> = BTreeMap::new();
    for row in decode_rows::<FacilityRow>(FACILITIES, facilities)? {
        if !provided(&row.provided)? {
            continue;
        }
        let Some(station) = index.get(row.station_id) else {
            unknown_stations.record(row.station_id);
            continue;
        };
        let code: FacilityCode = parse(FACILITIES, "Key", &row.code)?;
        let Some(&rank) = rank.get(&code) else {
            unknown_facilities.record(code.as_str());
            continue;
        };
        let location = (!row.location_en.is_empty() || !row.location_tc.is_empty())
            .then(|| clean_localized(&row.location_en, &row.location_tc));
        stations
            .entry(station)
            .or_default()
            .push((rank, StationFacility { code, location }));
    }
    unknown_stations.report();
    unknown_facilities.report();

    Ok(Accessibility {
        categories,
        stations: stations
            .into_iter()
            .map(|(station, mut facilities)| {
                facilities.sort_by_key(|(rank, _)| *rank);
                facilities.dedup_by_key(|(rank, _)| *rank);
                StationAccessibility {
                    station,
                    facilities: facilities
                        .into_iter()
                        .map(|(_, facility)| facility)
                        .collect(),
                }
            })
            .collect(),
    })
}

/// The catalogue, grouped by category and ordered as the MTR orders it:
/// by its sorting order, then by position in the file where that repeats.
fn clean_catalogue(body: &[u8]) -> Result<Vec<FacilityGroup>, OpenDataError> {
    let mut unknown = Skipped::new(CATALOGUE, "unknown category");
    let mut groups: BTreeMap<FacilityCategory, FacilityGroup> = BTreeMap::new();
    let mut orders: HashMap<FacilityCode, u16> = HashMap::new();

    for row in decode_rows::<CategoryRow>(CATALOGUE, body)? {
        let Some(category) = category(&row.category) else {
            unknown.record(&row.category);
            continue;
        };
        let code: FacilityCode = parse(CATALOGUE, "Item_Code", &row.code)?;
        orders.insert(code.clone(), row.order);
        groups
            .entry(category)
            .or_insert_with(|| FacilityGroup {
                category,
                name: clean_localized(&row.category_en, &row.category_tc),
                facilities: Vec::new(),
            })
            .facilities
            .push(Facility {
                code,
                name: clean_localized(&row.name_en, &row.name_tc),
            });
    }
    unknown.report();

    let mut groups: Vec<_> = groups.into_values().collect();
    for group in &mut groups {
        // A stable sort keeps file order among equal sorting orders.
        group
            .facilities
            .sort_by_key(|facility| orders.get(&facility.code).copied());
        group
            .facilities
            .dedup_by(|later, earlier| later.code == earlier.code);
    }
    Ok(groups)
}

fn category(id: &str) -> Option<FacilityCategory> {
    match id {
        "AJ" => Some(FacilityCategory::StationAccess),
        "VJ" => Some(FacilityCategory::VisuallyImpaired),
        "HJ" => Some(FacilityCategory::HearingImpaired),
        "MJ" => Some(FacilityCategory::MobilityImpaired),
        _ => None,
    }
}

fn provided(value: &str) -> Result<bool, OpenDataError> {
    match value {
        "Y" => Ok(true),
        "N" => Ok(false),
        _ => Err(OpenDataError::UnexpectedValue {
            file: FACILITIES,
            column: "Value",
            value: value.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use dut_core::{domain::localized::Localized, station};

    use super::*;
    use crate::{
        fixtures,
        mtr::open_data::{fares, stations},
    };

    /// The station index as a real poll builds it, with Racecourse learned
    /// from the fare file.
    fn index() -> StationIndex {
        let (_, mut index) =
            stations::clean(&fixtures::read("mtr/open_data/mtr_lines_and_stations.csv"))
                .expect("the captured station list should clean");
        fares::clean_mtr(
            &fixtures::read("mtr/open_data/mtr_lines_fares.csv"),
            &mut index,
        )
        .expect("the captured fares should clean");
        index
    }

    fn accessibility() -> Accessibility {
        clean(
            &fixtures::read("mtr/open_data/barrier_free_facility_category.csv"),
            &fixtures::read("mtr/open_data/barrier_free_facilities.csv"),
            &index(),
        )
        .expect("the captured facilities should clean")
    }

    fn station(
        accessibility: &Accessibility,
        station: StationCode,
    ) -> Vec<(String, Option<String>)> {
        accessibility
            .stations
            .iter()
            .find(|entry| entry.station == station)
            .map(|entry| {
                entry
                    .facilities
                    .iter()
                    .map(|facility| {
                        let location = facility.location.as_ref().map(|text| text.en.clone());
                        (facility.code.as_str().to_owned(), location)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn groups_the_catalogue_by_category() {
        let accessibility = accessibility();
        let categories: Vec<_> = accessibility
            .categories
            .iter()
            .map(|group| group.category)
            .collect();

        assert_eq!(
            categories,
            [
                FacilityCategory::StationAccess,
                FacilityCategory::VisuallyImpaired,
                FacilityCategory::HearingImpaired,
                FacilityCategory::MobilityImpaired,
            ]
        );
        assert_eq!(
            accessibility.categories[0].name,
            Localized::new("System Accessibility".to_owned(), "出入口設施".to_owned())
        );
        let facilities: usize = accessibility
            .categories
            .iter()
            .map(|group| group.facilities.len())
            .sum();
        assert_eq!(facilities, 36);
    }

    #[test]
    fn decodes_character_references_in_names() {
        let accessibility = accessibility();
        let flashing_maps = accessibility
            .categories
            .iter()
            .flat_map(|group| &group.facilities)
            .find(|facility| facility.code.as_str() == "HJ3")
            .expect("HJ3 is in the catalogue");

        assert_eq!(flashing_maps.name.tc, "閃燈路綫圖");
    }

    #[test]
    fn keeps_file_order_where_sorting_orders_repeat() {
        let accessibility = accessibility();
        let mobility: Vec<_> = accessibility.categories[3]
            .facilities
            .iter()
            .map(|facility| facility.code.as_str())
            .collect();

        assert_eq!(
            mobility,
            ["MJ1", "MJ2", "MJ3", "MJ4", "MJ5", "MJ6", "MJ8", "MJ7"]
        );
    }

    #[test]
    fn lists_only_what_a_station_provides_with_its_location() {
        let accessibility = accessibility();
        let admiralty = station(&accessibility, station!("ADM"));

        assert!(!admiralty.is_empty());
        assert!(
            admiralty
                .iter()
                .any(|(code, location)| code.starts_with("AJ") && location.is_some())
        );
        assert!(admiralty.windows(2).all(|pair| pair[0].0 != pair[1].0));
    }

    #[test]
    fn resolves_racecourse_through_the_fare_file() {
        assert!(!station(&accessibility(), station!("RAC")).is_empty());
    }

    #[test]
    fn rejects_an_unexpected_availability() {
        let catalogue = fixtures::read("mtr/open_data/barrier_free_facility_category.csv");
        let facilities =
            "Station_No,Key,Value,AJTextEn,AJTextZh,Exit_Coordinate_X_Y\n1,AJ1,Maybe,,,\n";

        let result = clean(&catalogue, facilities.as_bytes(), &index());

        assert!(matches!(
            result,
            Err(OpenDataError::UnexpectedValue {
                column: "Value",
                ..
            })
        ));
    }
}
