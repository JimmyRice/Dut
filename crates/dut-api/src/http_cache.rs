//! `Cache-Control` for API responses, so downstream HTTP caches (a CDN, the
//! app's URL cache) can reuse responses exactly as long as they stay valid.

use axum::{
    Json,
    http::{HeaderValue, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use dut_core::application::source::Freshness;

/// For data compiled into the service, which only changes on deployment.
pub(super) fn reference_data() -> HeaderValue {
    HeaderValue::from_static("public, max-age=86400")
}

/// Fresh data may be reused for its remaining lifetime; stale data must not
/// be reused at all.
pub(super) fn for_freshness(freshness: Freshness) -> HeaderValue {
    match freshness {
        Freshness::Fresh { expires_in } => {
            HeaderValue::try_from(format!("public, max-age={}", expires_in.as_secs()))
                .unwrap_or_else(|_| no_cache())
        }
        Freshness::Stale => no_cache(),
    }
}

/// For answers that must come from the service itself every time, such as
/// a health check, never from a cache in between.
pub(super) fn no_store() -> HeaderValue {
    HeaderValue::from_static("no-store")
}

/// A JSON response with the given `Cache-Control` header.
pub(super) fn json(cache_control: HeaderValue, body: impl Serialize) -> Response {
    ([(CACHE_CONTROL, cache_control)], Json(body)).into_response()
}

fn no_cache() -> HeaderValue {
    HeaderValue::from_static("no-cache")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn fresh_data_is_cacheable_for_its_remaining_whole_seconds() {
        let freshness = Freshness::Fresh {
            expires_in: Duration::from_millis(7_900),
        };

        assert_eq!(for_freshness(freshness), "public, max-age=7");
    }

    #[test]
    fn stale_data_is_not_cacheable() {
        assert_eq!(for_freshness(Freshness::Stale), "no-cache");
    }
}
