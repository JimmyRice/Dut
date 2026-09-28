use axum::{
    Router,
    http::{StatusCode, header::CACHE_CONTROL},
    response::IntoResponse,
    routing::get,
};

use crate::http_cache;

/// The liveness probe for load balancers and orchestrators.
///
/// It answers as long as the process serves HTTP, and reads neither
/// upstreams nor state, so an upstream outage never gets a healthy process
/// restarted. `200` rather than `204` because load balancers such as AWS ALB,
/// Google Cloud, and Cloudflare expect `200` by default.
pub(crate) fn router() -> Router {
    Router::new().route("/api/health", get(health))
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, [(CACHE_CONTROL, http_cache::no_store())])
}
