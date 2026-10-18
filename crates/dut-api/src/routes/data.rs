//! MTR open data: the index, each cleaned dataset, and each file as
//! published. Every response carries an `ETag`, and a client that sends it
//! back in `If-None-Match` gets `304 Not Modified` until the data changes.

use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    response::Response,
    routing::get,
};

use dut_core::{
    application::{
        line_status::LineStatusSource, next_train::NextTrainSource,
        reference_data::ReferenceDataSource, source::Snapshot,
    },
    domain::reference::{BySourceFile, Dataset, ReferenceData, Revision, SourceFile},
};

use crate::{
    body_version::FetchVersion,
    dto::{
        AccessibilityResponse, DataIndexResponse, LightRailResponse, PublishedStationsResponse,
        fares,
    },
    encoded_body::{EncodedBody, JSON, render_json},
    error::ApiError,
    http_cache::{self, EntityTag},
    state::AppState,
};

/// The files are UTF-8, as the portal serves them.
const CSV: HeaderValue = HeaderValue::from_static("text/csv; charset=utf-8");

/// Every open data body, each encoded once per version and shared by the
/// requests for it.
#[derive(Debug)]
pub(crate) struct OpenDataBodies {
    stations: EncodedBody<FetchVersion>,
    fares: EncodedBody<FetchVersion>,
    airport_express_fares: EncodedBody<FetchVersion>,
    light_rail: EncodedBody<FetchVersion>,
    light_rail_fares: EncodedBody<FetchVersion>,
    accessibility: EncodedBody<FetchVersion>,
    /// A file's body depends on its bytes alone.
    files: BySourceFile<EncodedBody<Revision>>,
}

impl Default for OpenDataBodies {
    fn default() -> Self {
        Self {
            stations: EncodedBody::new("stations"),
            fares: EncodedBody::new("fares"),
            airport_express_fares: EncodedBody::new("airport-express-fares"),
            light_rail: EncodedBody::new("light-rail"),
            light_rail_fares: EncodedBody::new("light-rail-fares"),
            accessibility: EncodedBody::new("accessibility"),
            files: BySourceFile::from_fn(|file| EncodedBody::new(file.file_name())),
        }
    }
}

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
    let content = Arc::clone(published.body());

    Ok(http_cache::conditional(
        &headers,
        &EntityTag::file(published.revision()),
        http_cache::for_freshness(snapshot.freshness()),
        state.open_data_bodies().files.get(file).respond(
            &headers,
            published.revision(),
            CSV,
            move || Ok(Bytes::from_owner(content)),
        ),
    )
    .await)
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
        &state.open_data_bodies().stations,
        |snapshot| render_json(&PublishedStationsResponse::from(snapshot)),
    )
    .await)
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
        &state.open_data_bodies().fares,
        |snapshot| render_json(&fares::mtr(snapshot)),
    )
    .await)
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
        &state.open_data_bodies().airport_express_fares,
        |snapshot| render_json(&fares::airport_express(snapshot)),
    )
    .await)
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
        &state.open_data_bodies().light_rail,
        |snapshot| render_json(&LightRailResponse::from(snapshot)),
    )
    .await)
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
        &state.open_data_bodies().light_rail_fares,
        |snapshot| render_json(&fares::light_rail(snapshot)),
    )
    .await)
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
        &state.open_data_bodies().accessibility,
        |snapshot| render_json(&AccessibilityResponse::from(snapshot)),
    )
    .await)
}

/// One cleaned dataset as JSON, or `304` when the client already has this
/// revision, in which case the body is never encoded.
async fn dataset<T>(
    headers: &HeaderMap,
    snapshot: &Snapshot<ReferenceData>,
    dataset: &Dataset<T>,
    body: &EncodedBody<FetchVersion>,
    render: impl FnOnce(&Snapshot<ReferenceData>) -> Result<Bytes, serde_json::Error> + Send + 'static,
) -> Response {
    let source = snapshot.clone();
    http_cache::conditional(
        headers,
        &EntityTag::dataset(dataset.revision()),
        http_cache::for_freshness(snapshot.freshness()),
        body.respond(headers, FetchVersion::of(snapshot), JSON, move || {
            render(&source)
        }),
    )
    .await
}
