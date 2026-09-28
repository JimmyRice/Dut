use serde::Serialize;

use dut_core::{
    application::source::Snapshot,
    domain::reference::{
        FacilityCategory, FacilityGroup, ReferenceData, StationAccessibility, StationFacility,
    },
};

use crate::dto::common::{DatasetMeta, LocalizedText, StationRef};

/// `GET /api/data/accessibility`: the MTR's catalogue of barrier-free
/// facilities, and what each station provides.
#[derive(Debug, Serialize)]
pub(crate) struct AccessibilityResponse<'a> {
    #[serde(flatten)]
    pub meta: DatasetMeta,
    pub categories: Vec<CategoryBody<'a>>,
    pub stations: Vec<StationAccessibilityBody<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CategoryBody<'a> {
    /// `station_access`, `visually_impaired`, `hearing_impaired`, or
    /// `mobility_impaired`.
    pub category: &'static str,
    pub name: LocalizedText<'a>,
    pub facilities: Vec<FacilityBody<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FacilityBody<'a> {
    pub code: &'a str,
    pub name: LocalizedText<'a>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StationAccessibilityBody<'a> {
    pub station: StationRef<'a>,
    /// Only the facilities the station provides, in catalogue order.
    pub facilities: Vec<StationFacilityBody<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StationFacilityBody<'a> {
    pub code: &'a str,
    /// Where it is, such as "Exits A & D", when the MTR says.
    pub location: Option<LocalizedText<'a>>,
}

impl<'a> From<&'a Snapshot<ReferenceData>> for AccessibilityResponse<'a> {
    fn from(snapshot: &'a Snapshot<ReferenceData>) -> Self {
        let dataset = &snapshot.value().accessibility;
        let accessibility = dataset.value();
        Self {
            meta: DatasetMeta::new(snapshot, dataset),
            categories: accessibility
                .categories
                .iter()
                .map(CategoryBody::from)
                .collect(),
            stations: accessibility
                .stations
                .iter()
                .map(StationAccessibilityBody::from)
                .collect(),
        }
    }
}

impl<'a> From<&'a FacilityGroup> for CategoryBody<'a> {
    fn from(group: &'a FacilityGroup) -> Self {
        Self {
            category: category_code(group.category),
            name: (&group.name).into(),
            facilities: group
                .facilities
                .iter()
                .map(|facility| FacilityBody {
                    code: facility.code.as_str(),
                    name: (&facility.name).into(),
                })
                .collect(),
        }
    }
}

impl<'a> From<&'a StationAccessibility> for StationAccessibilityBody<'a> {
    fn from(station: &'a StationAccessibility) -> Self {
        Self {
            station: StationRef::from(&station.station),
            facilities: station
                .facilities
                .iter()
                .map(|facility: &'a StationFacility| StationFacilityBody {
                    code: facility.code.as_str(),
                    location: facility.location.as_ref().map(LocalizedText::from),
                })
                .collect(),
        }
    }
}

const fn category_code(category: FacilityCategory) -> &'static str {
    match category {
        FacilityCategory::StationAccess => "station_access",
        FacilityCategory::VisuallyImpaired => "visually_impaired",
        FacilityCategory::HearingImpaired => "hearing_impaired",
        FacilityCategory::MobilityImpaired => "mobility_impaired",
    }
}
