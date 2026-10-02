//! Simulated MTR data for the mock API, so the app can be built against
//! situations that are rare or inconvenient to wait for live: rush hour, the
//! last train, a delay, a typhoon signal, an upstream outage.
//!
//! The simulation follows how the network actually runs. Lines, stations,
//! branches, and termini come from the static network in `dut-core`;
//! platforms, short workings, and headways from the live Next Train API.
//! Boards come out in the same domain types as real ones, through the same
//! `NextTrainService`, so the mock API serves them with the real endpoints'
//! DTOs and errors.
//!
//! A [`Seed`] picks one simulated world. Trains in it pass each station at
//! fixed times, so polling a board shows them approaching and leaving, and
//! the boards of neighbouring stations agree. Nothing here performs I/O.

mod board;
mod network_status;
mod notices;
mod platforms;
mod scenario;
mod seed;
mod simulated;
mod timetable;

pub use scenario::{BoardScenario, Scenario, ScenarioChoice, StatusScenario, UnknownScenario};
pub use seed::Seed;
pub use simulated::{SimulatedLineStatus, SimulatedNextTrains};
