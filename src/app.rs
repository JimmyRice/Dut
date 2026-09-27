use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{Request, Response},
};
use tower_http::{
    LatencyUnit,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultOnRequest, DefaultOnResponse, OnResponse, TraceLayer},
};
use tracing::{Level, Span, field, info_span};

use crate::{api, infrastructure::millis, state::AppState, telemetry::REQUEST_SPAN};

const REQUEST_ID_HEADER: &str = "x-request-id";

pub(crate) fn build_router(state: AppState) -> Router {
    // Layers run outermost-last: the request ID is assigned first, so the
    // trace span can include it, and it is echoed back on the response.
    Router::new()
        .nest("/api", api::router())
        .fallback(api::not_found)
        .with_state(state)
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(record_response),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

/// One span per inbound request, so every log line it causes, including
/// cache and upstream activity, carries the request ID. The terminal logger
/// groups those lines under a header built from these fields.
fn request_span(request: &Request<Body>) -> Span {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");
    info_span!(
        REQUEST_SPAN,
        id = %request_id,
        method = %request.method(),
        path = %request.uri().path(),
        status = field::Empty,
        latency_ms = field::Empty,
    )
}

/// Logs the response and records its outcome on the request span, so the
/// request's log block can open with a one-line summary.
fn record_response(response: &Response<Body>, latency: Duration, span: &Span) {
    span.record("status", response.status().as_u16());
    span.record("latency_ms", millis(latency));
    DefaultOnResponse::new()
        .level(Level::INFO)
        .latency_unit(LatencyUnit::Millis)
        .on_response(response, latency, span);
}
