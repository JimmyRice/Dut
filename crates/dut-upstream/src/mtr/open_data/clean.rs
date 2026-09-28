//! Turns one poll's files into every dataset, in the order their
//! dependencies require.

use tracing::{debug, info, warn};

use dut_core::domain::reference::{
    BySourceFile, Dataset, NetworkDrift, PublishedFile, PublishedNetwork, ReferenceData,
};

use crate::mtr::open_data::{accessibility, error::OpenDataError, fares, light_rail, stations};

/// Cleans every dataset from `files`, which it then keeps as the published
/// copies.
///
/// The station list comes first because the fare and facility files name
/// stations by ID; the MTR fare file comes before the facility file because
/// it teaches the index Racecourse's ID.
pub(super) fn reference_data(
    files: BySourceFile<PublishedFile>,
) -> Result<ReferenceData, OpenDataError> {
    let (network, mut index) = stations::clean(files.lines_and_stations.body())?;
    let fares = fares::clean_mtr(files.lines_fares.body(), &mut index)?;
    let airport_express_fares =
        fares::clean_airport_express(files.airport_express_fares.body(), &mut index)?;
    let light_rail = light_rail::clean(files.light_rail_routes_and_stops.body())?;
    let light_rail_fares = fares::clean_light_rail(files.light_rail_fares.body(), &light_rail)?;
    let accessibility = accessibility::clean(
        files.barrier_free_facility_categories.body(),
        files.barrier_free_facilities.body(),
        &index,
    )?;
    report_drift(&network);

    let accessibility_updated_at = files
        .barrier_free_facility_categories
        .updated_at()
        .max(files.barrier_free_facilities.updated_at());
    Ok(ReferenceData {
        stations: Dataset::new(network, files.lines_and_stations.updated_at()),
        fares: Dataset::new(fares, files.lines_fares.updated_at()),
        airport_express_fares: Dataset::new(
            airport_express_fares,
            files.airport_express_fares.updated_at(),
        ),
        light_rail: Dataset::new(light_rail, files.light_rail_routes_and_stops.updated_at()),
        light_rail_fares: Dataset::new(light_rail_fares, files.light_rail_fares.updated_at()),
        accessibility: Dataset::new(accessibility, accessibility_updated_at),
        files,
    })
}

/// Logs where open data disagrees with the compiled network, so a new or
/// renamed station prompts an update of the compiled network.
fn report_drift(network: &PublishedNetwork) {
    const HINT: &str = "run scripts/sync-network.py to update the compiled network";
    let drift = network.drift();
    for difference in &drift {
        match difference {
            NetworkDrift::UnknownStation(station) => {
                warn!(%station, hint = HINT, "open data lists a station this service does not know");
            }
            NetworkDrift::Renamed { station, published } => warn!(
                %station,
                en = %published.en,
                tc = %published.tc,
                hint = HINT,
                "open data names a station differently"
            ),
            NetworkDrift::LineStations {
                line,
                added,
                removed,
            } => warn!(
                %line,
                added = ?added,
                removed = ?removed,
                hint = HINT,
                "open data lists different stations on a line"
            ),
            NetworkDrift::Unpublished(station) => {
                debug!(%station, "open data leaves out a station this service knows");
            }
        }
    }
    let unexpected = drift
        .iter()
        .filter(|difference| !matches!(difference, NetworkDrift::Unpublished(_)))
        .count();
    if unexpected == 0 {
        info!("open data agrees with the compiled network");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dut_core::domain::{network::StationCode, reference::SourceFile};
    use jiff::Timestamp;

    use super::*;
    use crate::fixtures;

    fn captured(file: SourceFile) -> PublishedFile {
        let body = fixtures::read(&format!("mtr/open_data/{}", file.file_name()));
        let updated_at = match file {
            SourceFile::BarrierFreeFacilities => "2023-06-24T18:28:09Z",
            _ => "2023-06-24T18:28:08Z",
        };
        PublishedFile::new(Arc::from(body), updated_at.parse::<Timestamp>().ok())
    }

    #[test]
    fn cleans_every_dataset_from_captured_files() {
        let data = reference_data(BySourceFile::from_fn(captured))
            .expect("the captured files should clean");

        assert_eq!(data.stations.value().stations.len(), 97);
        assert!(!data.fares.value().trips().is_empty());
        assert!(!data.airport_express_fares.value().trips().is_empty());
        assert_eq!(data.light_rail.value().stops.len(), 68);
        assert!(!data.light_rail_fares.value().trips().is_empty());
        let racecourse: StationCode = "RAC".parse().expect("valid code");
        assert!(
            data.accessibility
                .value()
                .stations
                .iter()
                .any(|station| station.station == racecourse)
        );
    }

    #[test]
    fn keeps_the_files_byte_for_byte() {
        let data = reference_data(BySourceFile::from_fn(captured))
            .expect("the captured files should clean");

        for file in SourceFile::ALL {
            let expected = fixtures::read(&format!("mtr/open_data/{}", file.file_name()));
            assert_eq!(
                &**data.files.get(file).body(),
                expected.as_slice(),
                "{file}"
            );
        }
    }

    #[test]
    fn dates_a_dataset_by_the_latest_of_its_files() {
        let data = reference_data(BySourceFile::from_fn(captured))
            .expect("the captured files should clean");

        assert_eq!(
            data.accessibility.updated_at(),
            "2023-06-24T18:28:09Z".parse().ok()
        );
    }
}
