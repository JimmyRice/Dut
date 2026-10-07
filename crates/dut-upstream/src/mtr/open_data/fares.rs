//! The fare files: `mtr_lines_fares.csv`, `airport_express_fares.csv`, and
//! `light_rail_fares.csv`.

use serde::Deserialize;

use dut_core::domain::{
    network::StationCode,
    reference::{
        AdultAndChildFares, AirportExpressFares, Fare, FareTable, LightRailNetwork, OctopusFares,
        RailFares, SingleJourneyFares, SourceFile, StopId, Trip,
    },
};

use crate::mtr::open_data::{
    decode::{decode_rows, parse},
    error::OpenDataError,
    skipped::Skipped,
    station_index::StationIndex,
};

/// A trip on the MTR's heavy rail lines, between stations named by ID.
#[derive(Debug, Deserialize)]
struct MtrFareRow {
    #[serde(rename = "SRC_STATION_NAME")]
    from_name: String,
    #[serde(rename = "SRC_STATION_ID")]
    from_id: u16,
    #[serde(rename = "DEST_STATION_NAME")]
    to_name: String,
    #[serde(rename = "DEST_STATION_ID")]
    to_id: u16,
    #[serde(rename = "OCT_ADT_FARE")]
    octopus_adult: String,
    #[serde(rename = "OCT_STD_FARE")]
    octopus_student: String,
    #[serde(rename = "OCT_JOYYOU_SIXTY_FARE")]
    octopus_joyyou_sixty: String,
    #[serde(rename = "OCT_CON_CHILD_FARE")]
    octopus_child: String,
    #[serde(rename = "OCT_CON_ELDERLY_FARE")]
    octopus_elderly: String,
    #[serde(rename = "OCT_CON_PWD_FARE")]
    octopus_disability: String,
    #[serde(rename = "SINGLE_ADT_FARE")]
    single_adult: String,
    #[serde(rename = "SINGLE_CON_CHILD_FARE")]
    single_child: String,
    #[serde(rename = "SINGLE_CON_ELDERLY_FARE")]
    single_elderly: String,
}

#[derive(Debug, Deserialize)]
struct AirportExpressFareRow {
    #[serde(rename = "ST_FROM")]
    from_name: String,
    #[serde(rename = "ST_FROM_ID")]
    from_id: u16,
    #[serde(rename = "ST_TO")]
    to_name: String,
    #[serde(rename = "ST_TO_ID")]
    to_id: u16,
    #[serde(rename = "OCT_ADT_FARE")]
    octopus_adult: String,
    #[serde(rename = "OCT_CHD_FARE")]
    octopus_child: String,
    #[serde(rename = "SINGLE_ADT_FARE")]
    single_adult: String,
    #[serde(rename = "SINGLE_CHD_FARE")]
    single_child: String,
}

#[derive(Debug, Deserialize)]
struct LightRailFareRow {
    from_station_id: u16,
    to_station_id: u16,
    fare_octo_adult: String,
    fare_octo_student: String,
    fare_octo_joyyou_sixty: String,
    fare_octo_child: String,
    fare_octo_elderly: String,
    fare_octo_pwd: String,
    fare_single_adult: String,
    fare_single_child: String,
    fare_single_elderly: String,
}

/// MTR fares keyed by station code. IDs the station list leaves out, such
/// as Racecourse's, are resolved by name and remembered in `index`.
pub(super) fn clean_mtr(
    body: &[u8],
    index: &mut StationIndex,
) -> Result<FareTable<StationCode, RailFares>, OpenDataError> {
    const FILE: SourceFile = SourceFile::LinesFares;
    let fare = |column, value: &str| parse::<Fare>(FILE, column, value);
    let mut unresolved = Skipped::new(FILE, "unknown station ID");

    let mut trips = Vec::new();
    for row in decode_rows::<MtrFareRow>(FILE, body)? {
        let from = index.learn(row.from_id, &row.from_name);
        let to = index.learn(row.to_id, &row.to_name);
        let (Some(from), Some(to)) = (from, to) else {
            unresolved.record(format!("{} → {}", row.from_id, row.to_id));
            continue;
        };
        let fares = RailFares {
            octopus: OctopusFares {
                adult: fare("OCT_ADT_FARE", &row.octopus_adult)?,
                student: fare("OCT_STD_FARE", &row.octopus_student)?,
                joyyou_sixty: fare("OCT_JOYYOU_SIXTY_FARE", &row.octopus_joyyou_sixty)?,
                child: fare("OCT_CON_CHILD_FARE", &row.octopus_child)?,
                elderly: fare("OCT_CON_ELDERLY_FARE", &row.octopus_elderly)?,
                disability: fare("OCT_CON_PWD_FARE", &row.octopus_disability)?,
            },
            single_journey: SingleJourneyFares {
                adult: fare("SINGLE_ADT_FARE", &row.single_adult)?,
                child: fare("SINGLE_CON_CHILD_FARE", &row.single_child)?,
                elderly: fare("SINGLE_CON_ELDERLY_FARE", &row.single_elderly)?,
            },
        };
        trips.push(Trip { from, to, fares });
    }
    unresolved.report();
    Ok(FareTable::new(trips))
}

/// Airport Express fares keyed by station code. The file uses the Airport
/// Express's own IDs for Hong Kong, Kowloon, and Tsing Yi.
pub(super) fn clean_airport_express(
    body: &[u8],
    index: &mut StationIndex,
) -> Result<FareTable<StationCode, AirportExpressFares>, OpenDataError> {
    const FILE: SourceFile = SourceFile::AirportExpressFares;
    let fare = |column, value: &str| parse::<Fare>(FILE, column, value);
    let mut unresolved = Skipped::new(FILE, "unknown station ID");

    let mut trips = Vec::new();
    for row in decode_rows::<AirportExpressFareRow>(FILE, body)? {
        let from = index.learn(row.from_id, &row.from_name);
        let to = index.learn(row.to_id, &row.to_name);
        let (Some(from), Some(to)) = (from, to) else {
            unresolved.record(format!("{} → {}", row.from_id, row.to_id));
            continue;
        };
        let fares = AirportExpressFares {
            octopus: AdultAndChildFares {
                adult: fare("OCT_ADT_FARE", &row.octopus_adult)?,
                child: fare("OCT_CHD_FARE", &row.octopus_child)?,
            },
            single_journey: AdultAndChildFares {
                adult: fare("SINGLE_ADT_FARE", &row.single_adult)?,
                child: fare("SINGLE_CHD_FARE", &row.single_child)?,
            },
        };
        trips.push(Trip { from, to, fares });
    }
    unresolved.report();
    Ok(FareTable::new(trips))
}

/// Light Rail fares keyed by stop number, for stops on `network`.
pub(super) fn clean_light_rail(
    body: &[u8],
    network: &LightRailNetwork,
) -> Result<FareTable<StopId, RailFares>, OpenDataError> {
    const FILE: SourceFile = SourceFile::LightRailFares;
    let fare = |column, value: &str| parse::<Fare>(FILE, column, value);
    let mut unknown = Skipped::new(FILE, "unknown stop ID");

    let mut trips = Vec::new();
    for row in decode_rows::<LightRailFareRow>(FILE, body)? {
        let from = StopId::new(row.from_station_id);
        let to = StopId::new(row.to_station_id);
        if network.stop(from).is_none() || network.stop(to).is_none() {
            unknown.record(format!("{from} → {to}"));
            continue;
        }
        let fares = RailFares {
            octopus: OctopusFares {
                adult: fare("fare_octo_adult", &row.fare_octo_adult)?,
                student: fare("fare_octo_student", &row.fare_octo_student)?,
                joyyou_sixty: fare("fare_octo_joyyou_sixty", &row.fare_octo_joyyou_sixty)?,
                child: fare("fare_octo_child", &row.fare_octo_child)?,
                elderly: fare("fare_octo_elderly", &row.fare_octo_elderly)?,
                disability: fare("fare_octo_pwd", &row.fare_octo_pwd)?,
            },
            single_journey: SingleJourneyFares {
                adult: fare("fare_single_adult", &row.fare_single_adult)?,
                child: fare("fare_single_child", &row.fare_single_child)?,
                elderly: fare("fare_single_elderly", &row.fare_single_elderly)?,
            },
        };
        trips.push(Trip { from, to, fares });
    }
    unknown.report();
    Ok(FareTable::new(trips))
}

#[cfg(test)]
mod tests {
    use dut_core::station;

    use super::*;
    use crate::{
        fixtures,
        mtr::open_data::{light_rail, stations},
    };

    // The fare fixtures are captured files trimmed to trips between a few
    // stops: Central, Admiralty, Hong Kong, Tseung Kwan O, LOHAS Park,
    // Fo Tan, and Racecourse for the MTR, and five Light Rail stops.

    fn index() -> StationIndex {
        let (_, index) =
            stations::clean(&fixtures::read("mtr/open_data/mtr_lines_and_stations.csv"))
                .expect("the captured station list should clean");
        index
    }

    fn cents(fare: Fare) -> u32 {
        fare.cents()
    }

    #[test]
    fn keys_mtr_fares_by_station_code_in_cents() {
        let mut index = index();
        let table = clean_mtr(
            &fixtures::read("mtr/open_data/mtr_lines_fares.csv"),
            &mut index,
        )
        .expect("the captured fares should clean");

        let fares = table
            .get(station!("CEN"), station!("ADM"))
            .expect("Central to Admiralty");
        assert_eq!(cents(fares.octopus.adult), 490);
        assert_eq!(cents(fares.octopus.student), 320);
        assert_eq!(cents(fares.octopus.joyyou_sixty), 200);
        assert_eq!(cents(fares.single_journey.adult), 500);
        assert_eq!(cents(fares.single_journey.child), 350);
        // Seven stations, every ordered pair except a station to itself.
        assert_eq!(table.trips().len(), 7 * 6);
    }

    #[test]
    fn resolves_racecourse_by_name_and_remembers_it() {
        let mut index = index();
        let table = clean_mtr(
            &fixtures::read("mtr/open_data/mtr_lines_fares.csv"),
            &mut index,
        )
        .expect("the captured fares should clean");

        assert!(table.get(station!("RAC"), station!("FOT")).is_some());
        assert_eq!(index.get(70), Some(station!("RAC")));
    }

    #[test]
    fn maps_airport_express_ids_to_shared_station_codes() {
        let table = clean_airport_express(
            &fixtures::read("mtr/open_data/airport_express_fares.csv"),
            &mut index(),
        )
        .expect("the captured Airport Express fares should clean");

        let fares = table
            .get(station!("KOW"), station!("AIR"))
            .expect("Kowloon to Airport");
        assert_eq!(cents(fares.octopus.adult), 10_500);
        assert_eq!(cents(fares.octopus.child), 5_250);
        assert_eq!(cents(fares.single_journey.adult), 11_500);
        assert_eq!(table.trips().len(), 14);
    }

    #[test]
    fn keys_light_rail_fares_by_stop_number() {
        let network = light_rail::clean(&fixtures::read(
            "mtr/open_data/light_rail_routes_and_stops.csv",
        ))
        .expect("the captured Light Rail network should clean");
        let table = clean_light_rail(
            &fixtures::read("mtr/open_data/light_rail_fares.csv"),
            &network,
        )
        .expect("the captured Light Rail fares should clean");

        let fares = table
            .get(StopId::new(1), StopId::new(10))
            .expect("Tuen Mun Ferry Pier to Melody Garden");
        assert_eq!(cents(fares.octopus.adult), 510);
        assert_eq!(cents(fares.single_journey.adult), 550);
        assert_eq!(table.trips().len(), 5 * 4);
    }

    #[test]
    fn leaves_out_trips_between_unknown_stations() {
        let body = "SRC_STATION_NAME,SRC_STATION_ID,DEST_STATION_NAME,DEST_STATION_ID,OCT_ADT_FARE,\
                    OCT_STD_FARE,OCT_JOYYOU_SIXTY_FARE,SINGLE_ADT_FARE,OCT_CON_CHILD_FARE,\
                    OCT_CON_ELDERLY_FARE,OCT_CON_PWD_FARE,SINGLE_CON_CHILD_FARE,SINGLE_CON_ELDERLY_FARE\n\
                    Nowhere,999,Central,1,5,5,5,5,5,5,5,5,5\n";

        let table = clean_mtr(body.as_bytes(), &mut index()).expect("the file should clean");

        assert!(table.trips().is_empty());
    }

    #[test]
    fn rejects_a_fare_that_is_not_an_amount() {
        let body = "ST_FROM,ST_FROM_ID,ST_TO,ST_TO_ID,OCT_ADT_FARE,OCT_CHD_FARE,SINGLE_ADT_FARE,SINGLE_CHD_FARE\n\
                    HongKong,44,Airport,47,free,60,130,65\n";

        let result = clean_airport_express(body.as_bytes(), &mut index());

        assert!(matches!(
            result,
            Err(OpenDataError::InvalidValue {
                column: "OCT_ADT_FARE",
                ..
            })
        ));
    }
}
