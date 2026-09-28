//! MTR open data: the index, each cleaned dataset, and each file as
//! published. Every response carries an `ETag`, and a client that sends it
//! back in `If-None-Match` gets `304 Not Modified` until the data changes.

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header::CONTENT_TYPE},
    response::Response,
    routing::get,
};
use serde::Serialize;

use dut_core::{
    application::{
        line_status::LineStatusSource, next_train::NextTrainSource,
        reference_data::ReferenceDataSource, source::Snapshot,
    },
    domain::reference::{Dataset, ReferenceData, SourceFile},
};

use crate::{
    dto::{
        AccessibilityResponse, DataIndexResponse, LightRailResponse, PublishedStationsResponse,
        fares,
    },
    error::ApiError,
    http_cache::{self, EntityTag},
    state::AppState,
};

/// The files are UTF-8, as the portal serves them.
const CSV: HeaderValue = HeaderValue::from_static("text/csv; charset=utf-8");

pub(crate) fn router<N: NextTrainSource, L: LineStatusSource, R: ReferenceDataSource>()
-> Router<AppState<N, L, R>> {
    Router::new()
        .route("/data", get(index::<N, L, R>))
        .route("/data/sources/{file}", get(source_file::<N, L, R>))
        .route("/data/stations", get(stations::<N, L, R>))
        .route("/data/fares", get(mtr_fares::<N, L, R>))
        .route(
            "/data/airport-express-fares",
            get(airport_express_fares::<N, L, R>),
        )
        .route("/data/light-rail", get(light_rail::<N, L, R>))
        .route("/data/light-rail-fares", get(light_rail_fares::<N, L, R>))
        .route("/data/accessibility", get(accessibility::<N, L, R>))
}

async fn index<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;

    Ok(http_cache::json(
        http_cache::for_freshness(snapshot.freshness()),
        DataIndexResponse::from(&snapshot),
    ))
}

/// A file byte for byte, so a client that downloaded it from the portal can
/// download it from here instead by changing only the base URL.
async fn source_file<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    Path(file): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let file: SourceFile = file.parse().map_err(|_| ApiError::UnknownSource)?;
    let snapshot = state.reference_data().reference_data().await?;
    let published = snapshot.value().files.get(file);

    Ok(http_cache::conditional(
        &headers,
        &EntityTag::file(published.revision()),
        http_cache::for_freshness(snapshot.freshness()),
        || {
            (
                [(CONTENT_TYPE, CSV)],
                Bytes::from_owner(Arc::clone(published.body())),
            )
        },
    ))
}

async fn stations<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;
    Ok(dataset(
        &headers,
        &snapshot,
        &snapshot.value().stations,
        || PublishedStationsResponse::from(&snapshot),
    ))
}

async fn mtr_fares<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;
    Ok(dataset(
        &headers,
        &snapshot,
        &snapshot.value().fares,
        || fares::mtr(&snapshot),
    ))
}

async fn airport_express_fares<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;
    Ok(dataset(
        &headers,
        &snapshot,
        &snapshot.value().airport_express_fares,
        || fares::airport_express(&snapshot),
    ))
}

async fn light_rail<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;
    Ok(dataset(
        &headers,
        &snapshot,
        &snapshot.value().light_rail,
        || LightRailResponse::from(&snapshot),
    ))
}

async fn light_rail_fares<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;
    Ok(dataset(
        &headers,
        &snapshot,
        &snapshot.value().light_rail_fares,
        || fares::light_rail(&snapshot),
    ))
}

async fn accessibility<N, L, R: ReferenceDataSource>(
    State(state): State<AppState<N, L, R>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = state.reference_data().reference_data().await?;
    Ok(dataset(
        &headers,
        &snapshot,
        &snapshot.value().accessibility,
        || AccessibilityResponse::from(&snapshot),
    ))
}

/// One cleaned dataset as JSON, or `304` when the client already has this
/// revision, in which case the body is never built.
fn dataset<T, B: Serialize>(
    headers: &HeaderMap,
    snapshot: &Snapshot<ReferenceData>,
    dataset: &Dataset<T>,
    body: impl FnOnce() -> B,
) -> Response {
    http_cache::conditional(
        headers,
        &EntityTag::dataset(dataset.revision()),
        http_cache::for_freshness(snapshot.freshness()),
        || Json(body()),
    )
}
