//! The MTR's open data portal: CSV files for the station list, fares, Light
//! Rail routes, and barrier-free facilities.
//!
//! [`MtrOpenDataFeed`] reads every file in one poll and cleans each into a
//! dataset: station IDs become station codes, fares become whole cents,
//! and the files' encoding defects (a byte order mark, blank rows, HTML
//! character references, a variant character in 荔) are fixed. The files
//! themselves are kept byte for byte, to be served as published.

mod accessibility;
mod clean;
mod decode;
mod error;
mod fares;
mod feed;
mod light_rail;
mod skipped;
mod station_index;
mod stations;

pub use feed::MtrOpenDataFeed;
