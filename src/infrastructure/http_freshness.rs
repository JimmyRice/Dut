//! Reads upstream HTTP caching headers.

use std::time::Duration;

use reqwest::header::{AGE, CACHE_CONTROL, HeaderMap};

/// How much longer upstream considers its response fresh.
///
/// Follows RFC 9111 as a shared cache would: `s-maxage` wins over `max-age`,
/// the `Age` already spent in upstream caches is subtracted, and `no-cache`
/// or `no-store` mean the response is already due for revalidation. Returns
/// `None` when upstream states no lifetime.
pub fn ttl_hint(headers: &HeaderMap) -> Option<Duration> {
    let cache_control = headers.get(CACHE_CONTROL)?.to_str().ok()?;

    let mut max_age = None;
    let mut shared_max_age = None;
    for directive in cache_control.split(',').map(str::trim) {
        if directive.eq_ignore_ascii_case("no-cache") || directive.eq_ignore_ascii_case("no-store")
        {
            return Some(Duration::ZERO);
        }
        let Some((name, value)) = directive.split_once('=') else {
            continue;
        };
        let seconds = value.trim().trim_matches('"').parse::<u64>().ok();
        match name.trim() {
            name if name.eq_ignore_ascii_case("max-age") => max_age = seconds,
            name if name.eq_ignore_ascii_case("s-maxage") => shared_max_age = seconds,
            _ => {}
        }
    }

    let lifetime = shared_max_age.or(max_age)?;
    let age = headers
        .get(AGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);

    Some(Duration::from_secs(lifetime.saturating_sub(age)))
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;

    fn headers(cache_control: &'static str, age: Option<&'static str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache_control));
        if let Some(age) = age {
            headers.insert(AGE, HeaderValue::from_static(age));
        }
        headers
    }

    #[test]
    fn subtracts_the_age_spent_in_upstream_caches() {
        let headers = headers("public, must-revalidate, max-age=10", Some("2"));

        assert_eq!(ttl_hint(&headers), Some(Duration::from_secs(8)));
    }

    #[test]
    fn prefers_the_shared_cache_lifetime() {
        let headers = headers("max-age=5, s-maxage=30", None);

        assert_eq!(ttl_hint(&headers), Some(Duration::from_secs(30)));
    }

    #[test]
    fn treats_revalidation_directives_as_already_expired() {
        let headers = headers("no-cache, no-store, must-revalidate", None);

        assert_eq!(ttl_hint(&headers), Some(Duration::ZERO));
    }

    #[test]
    fn never_goes_below_zero() {
        let headers = headers("max-age=10", Some("15"));

        assert_eq!(ttl_hint(&headers), Some(Duration::ZERO));
    }

    #[test]
    fn returns_none_without_a_lifetime() {
        assert_eq!(ttl_hint(&HeaderMap::new()), None);
        assert_eq!(ttl_hint(&headers("public", None)), None);
        assert_eq!(ttl_hint(&headers("max-age=soon", None)), None);
    }
}
