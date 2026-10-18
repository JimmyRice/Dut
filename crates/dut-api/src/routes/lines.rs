use std::sync::LazyLock;

use axum::{Router, http::HeaderMap, response::Response, routing::get};

use dut_core::domain::reference::Revision;

use crate::{
    dto::LinesResponse,
    encoded_body::{EncodedBody, JSON, render_json},
    http_cache::{self, EntityTag},
};

/// The network is compiled in, so the body never changes while the service
/// runs: it is encoded on the first request for each coding and shared by
/// every request after. Being constant, it needs no router state.
static BODY: EncodedBody<()> = EncodedBody::new("lines");

/// The tag of [`BODY`], worked out once for the same reason.
static TAG: LazyLock<EntityTag> =
    LazyLock::new(|| EntityTag::dataset(Revision::of(&LinesResponse::from_network())));

/// Serves reference data only, so it works with any router state.
pub(crate) fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/lines", get(lines))
}

async fn lines(headers: HeaderMap) -> Response {
    http_cache::conditional(
        &headers,
        &TAG,
        http_cache::reference_data(),
        BODY.respond(&headers, (), JSON, || {
            render_json(&LinesResponse::from_network())
        }),
    )
    .await
}
