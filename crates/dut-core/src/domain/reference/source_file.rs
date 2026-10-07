use std::{fmt, sync::Arc};

use jiff::Timestamp;
use thiserror::Error;

use crate::domain::{reference::dataset::Revision, string_enum::string_enum};

string_enum! {
    /// A file on the MTR's open data portal that this service reads and
    /// mirrors.
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum SourceFile {
        LinesAndStations => "mtr_lines_and_stations.csv",
        LinesFares => "mtr_lines_fares.csv",
        AirportExpressFares => "airport_express_fares.csv",
        LightRailRoutesAndStops => "light_rail_routes_and_stops.csv",
        LightRailFares => "light_rail_fares.csv",
        BarrierFreeFacilityCategories => "barrier_free_facility_category.csv",
        BarrierFreeFacilities => "barrier_free_facilities.csv",
    }

    /// The file's name on the portal, such as `mtr_lines_fares.csv`.
    pub const fn file_name;
    unknown = UnknownSourceFile;
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

    /// Builds one value per file, in [`SourceFile::ALL`] order, stopping at
    /// the first error.
    pub fn try_from_fn<E>(mut value: impl FnMut(SourceFile) -> Result<T, E>) -> Result<Self, E> {
        Ok(Self {
            lines_and_stations: value(SourceFile::LinesAndStations)?,
            lines_fares: value(SourceFile::LinesFares)?,
            airport_express_fares: value(SourceFile::AirportExpressFares)?,
            light_rail_routes_and_stops: value(SourceFile::LightRailRoutesAndStops)?,
            light_rail_fares: value(SourceFile::LightRailFares)?,
            barrier_free_facility_categories: value(SourceFile::BarrierFreeFacilityCategories)?,
            barrier_free_facilities: value(SourceFile::BarrierFreeFacilities)?,
        })
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
        let failed = BySourceFile::try_from_fn(|file| match file {
            SourceFile::LightRailFares => Err(file),
            _ => Ok(file.file_name()),
        });
        assert_eq!(failed, Err(SourceFile::LightRailFares));
    }

    #[test]
    fn a_published_file_is_revised_by_its_bytes() {
        let file = PublishedFile::new(Arc::from(&b"a,b\n1,2\n"[..]), None);

        assert_eq!(file.revision(), Revision::of_bytes(b"a,b\n1,2\n"));
        assert_eq!(&**file.body(), b"a,b\n1,2\n");
    }
}
