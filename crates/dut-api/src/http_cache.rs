//! `Cache-Control` and entity tags for API responses, so downstream HTTP
//! caches (a CDN, the app's URL cache) can reuse responses exactly as long as
//! they stay valid, and revalidate them without downloading them again.

use axum::{
    Json,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH},
    },
    response::{IntoResponse, Response},
};
use serde::Serialize;

use dut_core::{application::source::Freshness, domain::reference::Revision};

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

/// Identifies one version of a response body, sent as a weak `ETag`: gzip
/// may re-encode the body, so byte equality is not promised.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EntityTag(String);

impl EntityTag {
    /// For a cleaned dataset. The service's version is part of the tag, so
    /// a release that changes the JSON's shape invalidates clients' copies.
    pub(super) fn dataset(revision: Revision) -> Self {
        Self(format!("{}-{revision}", env!("CARGO_PKG_VERSION")))
    }

    /// For a file served byte for byte, which changes only upstream.
    pub(super) fn file(revision: Revision) -> Self {
        Self(revision.to_string())
    }

    /// The tag without quotes, as the data index lists it.
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }

    fn header_value(&self) -> HeaderValue {
        HeaderValue::try_from(format!("W/\"{}\"", self.0))
            .unwrap_or_else(|_| HeaderValue::from_static("W/\"\""))
    }

    /// Whether `If-None-Match` names this tag or `*`, compared weakly as
    /// RFC 9110 requires for that header.
    fn matches(&self, request: &HeaderMap) -> bool {
        request
            .get_all(IF_NONE_MATCH)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .any(|tag| {
                tag == "*"
                    || tag
                        .trim_start_matches("W/")
                        .strip_prefix('"')
                        .and_then(|tag| tag.strip_suffix('"'))
                        == Some(self.as_str())
            })
    }
}

/// Answers `304 Not Modified` when the client already holds `tag`, and
/// builds the body only otherwise. Both answers carry the tag and
/// `cache_control`, so a revalidated copy is reused for as long again.
pub(super) fn conditional<B: IntoResponse>(
    request: &HeaderMap,
    tag: &EntityTag,
    cache_control: HeaderValue,
    body: impl FnOnce() -> B,
) -> Response {
    let headers = [(CACHE_CONTROL, cache_control), (ETAG, tag.header_value())];
    if tag.matches(request) {
        (StatusCode::NOT_MODIFIED, headers).into_response()
    } else {
        (headers, body()).into_response()
    }
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

    fn if_none_match(value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static(value));
        headers
    }

    fn tag() -> EntityTag {
        EntityTag::file(Revision::of_bytes(b"a"))
    }

    #[test]
    fn sends_a_weak_tag() {
        assert_eq!(tag().header_value(), "W/\"af63dc4c8601ec8c\"");
        assert!(
            EntityTag::dataset(Revision::of_bytes(b"a"))
                .as_str()
                .ends_with("-af63dc4c8601ec8c")
        );
    }

    #[test]
    fn matches_the_tag_weak_or_strong_or_in_a_list() {
        for value in [
            "W/\"af63dc4c8601ec8c\"",
            "\"af63dc4c8601ec8c\"",
            "\"old\", W/\"af63dc4c8601ec8c\"",
            "*",
        ] {
            assert!(tag().matches(&if_none_match(value)), "{value}");
        }
    }

    #[test]
    fn does_not_match_other_tags() {
        for value in ["W/\"old\"", "af63dc4c8601ec8c", "W/\"af63dc4c8601ec8\""] {
            assert!(!tag().matches(&if_none_match(value)), "{value}");
        }
        assert!(!tag().matches(&HeaderMap::new()));
    }

    #[test]
    fn answers_not_modified_without_building_the_body() {
        let response = conditional(
            &if_none_match("W/\"af63dc4c8601ec8c\""),
            &tag(),
            HeaderValue::from_static("public, max-age=60"),
            || -> Json<()> { panic!("the body must not be built") },
        );

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()[CACHE_CONTROL], "public, max-age=60");
        assert_eq!(response.headers()[ETAG], "W/\"af63dc4c8601ec8c\"");
    }
}
