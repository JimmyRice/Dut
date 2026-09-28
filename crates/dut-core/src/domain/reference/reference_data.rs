use crate::domain::{
    network::StationCode,
    reference::{
        accessibility::Accessibility,
        dataset::Dataset,
        fare::{AirportExpressFares, FareTable, RailFares},
        light_rail::{LightRailNetwork, StopId},
        published_network::PublishedNetwork,
        source_file::{BySourceFile, PublishedFile},
    },
};

/// Everything read from MTR open data in one poll: each file as the MTR
/// published it, and the datasets cleaned from them.
///
/// Every part comes from the same read of every file, so the datasets agree
/// with each other and with the files: a fare's stations are always ones
/// the station list resolved.
#[derive(Debug)]
pub struct ReferenceData {
    pub files: BySourceFile<PublishedFile>,
    pub stations: Dataset<PublishedNetwork>,
    pub fares: Dataset<FareTable<StationCode, RailFares>>,
    pub airport_express_fares: Dataset<FareTable<StationCode, AirportExpressFares>>,
    pub light_rail: Dataset<LightRailNetwork>,
    pub light_rail_fares: Dataset<FareTable<StopId, RailFares>>,
    pub accessibility: Dataset<Accessibility>,
}
