//! Static reference data for the MTR network: lines, stations, and running
//! directions.

mod line;
mod station;

pub use line::{ByDirection, Direction, Line};
pub use station::{Station, StationCode};
