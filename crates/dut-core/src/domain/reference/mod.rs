//! Reference data the MTR publishes on its open data portal: the station
//! list, fares, Light Rail routes, and barrier-free facilities.
//!
//! It changes a few times a year, so it is read in the background and
//! served whole, both as the published files and as cleaned datasets, each
//! with a [`Revision`] that clients compare to skip downloads.

mod accessibility;
mod dataset;
mod fare;
mod light_rail;
mod published_network;
mod reference_data;
mod source_file;

pub use accessibility::{
    Accessibility, Facility, FacilityCategory, FacilityCode, FacilityGroup, InvalidFacilityCode,
    StationAccessibility, StationFacility,
};
pub use dataset::{Dataset, Revision};
pub use fare::{
    AdultAndChildFares, AirportExpressFares, Fare, FareTable, InvalidFare, OctopusFares, RailFares,
    SingleJourneyFares, Trip,
};
pub use light_rail::{
    InvalidRouteNumber, InvalidStopCode, LightRailNetwork, LightRailRoute, RouteNumber, Stop,
    StopCode, StopId,
};
pub use published_network::{NetworkDrift, PublishedNetwork, PublishedStation, Route};
pub use reference_data::ReferenceData;
pub use source_file::{BySourceFile, PublishedFile, SourceFile, UnknownSourceFile};
