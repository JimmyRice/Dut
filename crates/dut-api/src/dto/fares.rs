use serde::Serialize;

use dut_core::{
    application::source::Snapshot,
    domain::{
        network::StationCode,
        reference::{
            AdultAndChildFares, AirportExpressFares, Dataset, FareTable, OctopusFares, RailFares,
            ReferenceData, SingleJourneyFares, StopId, Trip,
        },
    },
};

use crate::dto::common::{DatasetMeta, MappedSeq};

/// `GET /api/data/fares`, `/api/data/airport-express-fares`, and
/// `/api/data/light-rail-fares`: every trip's fares, in whole Hong Kong
/// cents. `T` is the list of trips, serialized straight from the table.
#[derive(Debug, Serialize)]
pub(crate) struct FaresResponse<T> {
    #[serde(flatten)]
    pub meta: DatasetMeta,
    pub fares: T,
}

/// One trip. MTR trips name stations by code; Light Rail trips name stops
/// by number.
#[derive(Debug, Serialize)]
pub(crate) struct TripBody<K, F> {
    pub from: K,
    pub to: K,
    #[serde(flatten)]
    pub fares: F,
}

#[derive(Debug, Serialize)]
pub(crate) struct RailFaresBody {
    pub octopus: OctopusFaresBody,
    pub single_journey: SingleJourneyFaresBody,
}

#[derive(Debug, Serialize)]
pub(crate) struct OctopusFaresBody {
    pub adult: u32,
    pub student: u32,
    pub joyyou_sixty: u32,
    pub child: u32,
    pub elderly: u32,
    pub disability: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct SingleJourneyFaresBody {
    pub adult: u32,
    pub child: u32,
    pub elderly: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct AirportExpressFaresBody {
    pub octopus: AdultAndChildFaresBody,
    pub single_journey: AdultAndChildFaresBody,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdultAndChildFaresBody {
    pub adult: u32,
    pub child: u32,
}

/// MTR fares between stations.
pub(crate) fn mtr(snapshot: &Snapshot<ReferenceData>) -> FaresResponse<impl Serialize + '_> {
    let dataset = &snapshot.value().fares;
    response(snapshot, dataset, |trip: &Trip<StationCode, RailFares>| {
        TripBody {
            from: trip.from.as_str(),
            to: trip.to.as_str(),
            fares: RailFaresBody::from(&trip.fares),
        }
    })
}

/// Airport Express fares between its stations.
pub(crate) fn airport_express(
    snapshot: &Snapshot<ReferenceData>,
) -> FaresResponse<impl Serialize + '_> {
    let dataset = &snapshot.value().airport_express_fares;
    response(
        snapshot,
        dataset,
        |trip: &Trip<StationCode, AirportExpressFares>| TripBody {
            from: trip.from.as_str(),
            to: trip.to.as_str(),
            fares: AirportExpressFaresBody::from(&trip.fares),
        },
    )
}

/// Light Rail fares between stops.
pub(crate) fn light_rail(snapshot: &Snapshot<ReferenceData>) -> FaresResponse<impl Serialize + '_> {
    let dataset = &snapshot.value().light_rail_fares;
    response(snapshot, dataset, |trip: &Trip<StopId, RailFares>| {
        TripBody {
            from: trip.from.get(),
            to: trip.to.get(),
            fares: RailFaresBody::from(&trip.fares),
        }
    })
}

fn response<'a, K, F, B: Serialize>(
    snapshot: &'a Snapshot<ReferenceData>,
    dataset: &'a Dataset<FareTable<K, F>>,
    trip: impl Fn(&'a Trip<K, F>) -> B + 'a,
) -> FaresResponse<impl Serialize + 'a> {
    FaresResponse {
        meta: DatasetMeta::new(snapshot, dataset),
        fares: MappedSeq::new(dataset.value().trips(), trip),
    }
}

impl From<&RailFares> for RailFaresBody {
    fn from(fares: &RailFares) -> Self {
        let OctopusFares {
            adult,
            student,
            joyyou_sixty,
            child,
            elderly,
            disability,
        } = fares.octopus;
        let SingleJourneyFares {
            adult: single_adult,
            child: single_child,
            elderly: single_elderly,
        } = fares.single_journey;
        Self {
            octopus: OctopusFaresBody {
                adult: adult.cents(),
                student: student.cents(),
                joyyou_sixty: joyyou_sixty.cents(),
                child: child.cents(),
                elderly: elderly.cents(),
                disability: disability.cents(),
            },
            single_journey: SingleJourneyFaresBody {
                adult: single_adult.cents(),
                child: single_child.cents(),
                elderly: single_elderly.cents(),
            },
        }
    }
}

impl From<&AirportExpressFares> for AirportExpressFaresBody {
    fn from(fares: &AirportExpressFares) -> Self {
        Self {
            octopus: fares.octopus.into(),
            single_journey: fares.single_journey.into(),
        }
    }
}

impl From<AdultAndChildFares> for AdultAndChildFaresBody {
    fn from(fares: AdultAndChildFares) -> Self {
        Self {
            adult: fares.adult.cents(),
            child: fares.child.cents(),
        }
    }
}
