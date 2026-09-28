use std::{fmt, str::FromStr, sync::Arc};

use jiff::Timestamp;
use thiserror::Error;

use crate::domain::reference::dataset::Revision;

/// A file on the MTR's open data portal that this service reads and
/// mirrors.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceFile {
    LinesAndStations,
    LinesFares,
    AirportExpressFares,
    LightRailRoutesAndStops,
    LightRailFares,
    BarrierFreeFacilityCategories,
    BarrierFreeFacilities,
}

impl SourceFile {
    pub const ALL: [Self; 7] = [
        Self::LinesAndStations,
        Self::LinesFares,
        Self::AirportExpressFares,
        Self::LightRailRoutesAndStops,
        Self::LightRailFares,
        Self::BarrierFreeFacilityCategories,
        Self::BarrierFreeFacilities,
    ];

    /// The file's name on the portal, such as `mtr_lines_fares.csv`.
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::LinesAndStations => "mtr_lines_and_stations.csv",
            Self::LinesFares => "mtr_lines_fares.csv",
            Self::AirportExpressFares => "airport_express_fares.csv",
            Self::LightRailRoutesAndStops => "light_rail_routes_and_stops.csv",
            Self::LightRailFares => "light_rail_fares.csv",
            Self::BarrierFreeFacilityCategories => "barrier_free_facility_category.csv",
            Self::BarrierFreeFacilities => "barrier_free_facilities.csv",
        }
    }
}

/// Parses a file name as the portal spells it, case-insensitively.
impl FromStr for SourceFile {
    type Err = UnknownSourceFile;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|file| file.file_name().eq_ignore_ascii_case(input))
            .ok_or(UnknownSourceFile)
    }
}

impl fmt::Display for SourceFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.file_name())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("unknown MTR open data file")]
pub struct UnknownSourceFile;

/// One value per [`SourceFile`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BySourceFile<T> {
    pub lines_and_stations: T,
    pub lines_fares: T,
    pub airport_express_fares: T,
    pub light_rail_routes_and_stops: T,
    pub light_rail_fares: T,
    pub barrier_free_facility_categories: T,
    pub barrier_free_facilities: T,
}

impl<T> BySourceFile<T> {
    pub const fn get(&self, file: SourceFile) -> &T {
        match file {
            SourceFile::LinesAndStations => &self.lines_and_stations,
            SourceFile::LinesFares => &self.lines_fares,
            SourceFile::AirportExpressFares => &self.airport_express_fares,
            SourceFile::LightRailRoutesAndStops => &self.light_rail_routes_and_stops,
            SourceFile::LightRailFares => &self.light_rail_fares,
            SourceFile::BarrierFreeFacilityCategories => &self.barrier_free_facility_categories,
            SourceFile::BarrierFreeFacilities => &self.barrier_free_facilities,
        }
    }

    /// Builds one value per file, in [`SourceFile::ALL`] order.
    pub fn from_fn(mut value: impl FnMut(SourceFile) -> T) -> Self {
        Self {
            lines_and_stations: value(SourceFile::LinesAndStations),
            lines_fares: value(SourceFile::LinesFares),
            airport_express_fares: value(SourceFile::AirportExpressFares),
            light_rail_routes_and_stops: value(SourceFile::LightRailRoutesAndStops),
            light_rail_fares: value(SourceFile::LightRailFares),
            barrier_free_facility_categories: value(SourceFile::BarrierFreeFacilityCategories),
            barrier_free_facilities: value(SourceFile::BarrierFreeFacilities),
        }
    }

    pub fn map<U>(self, mut transform: impl FnMut(SourceFile, T) -> U) -> BySourceFile<U> {
        BySourceFile {
            lines_and_stations: transform(SourceFile::LinesAndStations, self.lines_and_stations),
            lines_fares: transform(SourceFile::LinesFares, self.lines_fares),
            airport_express_fares: transform(
                SourceFile::AirportExpressFares,
                self.airport_express_fares,
            ),
            light_rail_routes_and_stops: transform(
                SourceFile::LightRailRoutesAndStops,
                self.light_rail_routes_and_stops,
            ),
            light_rail_fares: transform(SourceFile::LightRailFares, self.light_rail_fares),
            barrier_free_facility_categories: transform(
                SourceFile::BarrierFreeFacilityCategories,
                self.barrier_free_facility_categories,
            ),
            barrier_free_facilities: transform(
                SourceFile::BarrierFreeFacilities,
                self.barrier_free_facilities,
            ),
        }
    }
}

/// A file exactly as the MTR published it, byte for byte, BOM included.
///
/// The bytes sit behind an [`Arc`], so serving the file to many clients
/// never copies it.
pub struct PublishedFile {
    body: Arc<[u8]>,
    updated_at: Option<Timestamp>,
    revision: Revision,
}

impl PublishedFile {
    pub fn new(body: Arc<[u8]>, updated_at: Option<Timestamp>) -> Self {
        Self {
            revision: Revision::of_bytes(&body),
            body,
            updated_at,
        }
    }

    pub const fn body(&self) -> &Arc<[u8]> {
        &self.body
    }

    /// From the file's `Last-Modified` header; `None` when upstream did not
    /// say.
    pub const fn updated_at(&self) -> Option<Timestamp> {
        self.updated_at
    }

    /// Changes exactly when the bytes change.
    pub const fn revision(&self) -> Revision {
        self.revision
    }
}

// Implemented by hand so that the bytes are summarised, not printed.
impl fmt::Debug for PublishedFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublishedFile")
            .field("bytes", &self.body.len())
            .field("updated_at", &self.updated_at)
            .field("revision", &self.revision)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_round_trip() {
        for file in SourceFile::ALL {
            assert_eq!(file.file_name().parse(), Ok(file));
        }
        assert_eq!("MTR_LINES_FARES.CSV".parse(), Ok(SourceFile::LinesFares));
        assert_eq!(
            "mtr_lines_fares".parse::<SourceFile>(),
            Err(UnknownSourceFile)
        );
        assert_eq!(
            "../etc/passwd".parse::<SourceFile>(),
            Err(UnknownSourceFile)
        );
    }

    #[test]
    fn by_source_file_reads_back_what_it_was_built_with() {
        let names = BySourceFile::from_fn(SourceFile::file_name);

        for file in SourceFile::ALL {
            assert_eq!(*names.get(file), file.file_name());
        }
        let lengths = names.map(|_, name| name.len());
        assert_eq!(lengths.lines_fares, "mtr_lines_fares.csv".len());
    }

    #[test]
    fn a_published_file_is_revised_by_its_bytes() {
        let file = PublishedFile::new(Arc::from(&b"a,b\n1,2\n"[..]), None);

        assert_eq!(file.revision(), Revision::of_bytes(b"a,b\n1,2\n"));
        assert_eq!(&**file.body(), b"a,b\n1,2\n");
    }
}
