use std::str::FromStr;

use thiserror::Error;

use crate::domain::{localized::Localized, network::StationCode};

/// Who a barrier-free facility serves, as the MTR groups its catalogue.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FacilityCategory {
    /// How to get between the street and the platform without stairs:
    /// lifts, ramps, and stair lifts. The MTR calls this "System
    /// Accessibility" (出入口設施).
    StationAccess,
    VisuallyImpaired,
    HearingImpaired,
    MobilityImpaired,
}

/// A facility's code in the MTR's catalogue, such as `AJ3` for a lift in
/// the unpaid area.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FacilityCode(Box<str>);

impl FacilityCode {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Accepts one to eight ASCII letters or digits, case preserved: the
/// catalogue has codes such as `VIn1`.
impl FromStr for FacilityCode {
    type Err = InvalidFacilityCode;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let valid = (1..=8).contains(&input.len())
            && input.bytes().all(|byte| byte.is_ascii_alphanumeric());
        valid.then(|| Self(input.into())).ok_or(InvalidFacilityCode)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a facility code must be one to eight ASCII letters or digits")]
pub struct InvalidFacilityCode;

/// One kind of facility in the catalogue.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Facility {
    pub code: FacilityCode,
    pub name: Localized<String>,
}

/// The facilities of one category, in the MTR's order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FacilityGroup {
    pub category: FacilityCategory,
    pub name: Localized<String>,
    pub facilities: Vec<Facility>,
}

/// A facility a station provides, and where, when the MTR says.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StationFacility {
    pub code: FacilityCode,
    /// Free text such as "Exits A & D", as published.
    pub location: Option<Localized<String>>,
}

/// The barrier-free facilities one station provides.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StationAccessibility {
    pub station: StationCode,
    /// Only those the station provides, in catalogue order.
    pub facilities: Vec<StationFacility>,
}

/// The MTR's barrier-free facilities: the catalogue, and which stations
/// provide what.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Accessibility {
    /// Ordered by [`FacilityCategory`].
    pub categories: Vec<FacilityGroup>,
    /// Ordered by station code.
    pub stations: Vec<StationAccessibility>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_facility_codes_as_published() {
        let code: FacilityCode = "VIn1".parse().expect("code should parse");

        assert_eq!(code.as_str(), "VIn1");
        for input in ["", "VIn1VIn1V", "AJ 3", "AJ-3"] {
            assert_eq!(
                input.parse::<FacilityCode>(),
                Err(InvalidFacilityCode),
                "{input}"
            );
        }
    }
}
