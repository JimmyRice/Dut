//! Which content coding a response goes out in, decided from the request's
//! `Accept-Encoding` exactly as tower-http's `CompressionLayer` decides it.
//!
//! A body compressed ahead of time carries its own `Content-Encoding`, so
//! the layer passes it through untouched. Deciding the same way the layer
//! does means a client gets gzip from a precompressed route exactly when it
//! would from any other route.

use axum::{
    body::Bytes,
    http::{
        HeaderMap, HeaderValue,
        header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE, VARY},
    },
    response::{IntoResponse, Response},
};

/// A coding the service sends: the compression layer supports gzip only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContentCoding {
    Identity,
    Gzip,
}

impl ContentCoding {
    /// The coding the compression layer would choose for this request.
    ///
    /// Gzip wins when its quality is above zero and at least identity's.
    /// Without a `*` entry, a coding's highest listed quality counts and an
    /// unlisted one counts as refused; with one, a coding's first listed
    /// quality counts and `*` stands in for an unlisted one. Entries with a
    /// malformed quality are ignored. When the client accepts neither
    /// coding, the layer answers `406 Not Acceptable` whatever the body, so
    /// identity serves as well as any.
    pub(crate) fn negotiate(request: &HeaderMap) -> Self {
        let mut identity = Listed::default();
        let mut gzip = Listed::default();
        let mut wildcard = None;
        for (coding, quality) in preferences(request) {
            if coding == "*" {
                wildcard = wildcard.or(Some(quality));
            } else if coding.eq_ignore_ascii_case("gzip") || coding.eq_ignore_ascii_case("x-gzip") {
                gzip.record(quality);
            } else if coding.eq_ignore_ascii_case("identity") {
                identity.record(quality);
            }
        }

        let (identity, gzip) = match wildcard {
            None => (identity.highest, gzip.highest),
            Some(any) => (identity.first.or(Some(any)), gzip.first.or(Some(any))),
        };
        let gzip = gzip.unwrap_or(Quality::REFUSED);
        if gzip > Quality::REFUSED && gzip >= identity.unwrap_or(Quality::REFUSED) {
            Self::Gzip
        } else {
            Self::Identity
        }
    }

    /// A response carrying `body`, already encoded in this coding.
    ///
    /// Both codings say they vary on `Accept-Encoding`, so caches in between
    /// keep them apart. The compression layer adds `Vary` only to responses
    /// it may compress itself, which excludes a precompressed one.
    pub(crate) fn respond(self, content_type: HeaderValue, body: Bytes) -> Response {
        let vary = (VARY, HeaderValue::from_static("accept-encoding"));
        match self {
            Self::Identity => ([(CONTENT_TYPE, content_type), vary], body).into_response(),
            Self::Gzip => (
                [
                    (CONTENT_TYPE, content_type),
                    vary,
                    (CONTENT_ENCODING, HeaderValue::from_static("gzip")),
                ],
                body,
            )
                .into_response(),
        }
    }
}

/// How much a client wants a coding, in thousandths: `q=0.5` is 500.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Quality(u16);

impl Quality {
    const REFUSED: Self = Self(0);
    const FULL: Self = Self(1000);

    /// Parses `q=` followed by 0 or 1 with up to three decimals (RFC 9110
    /// §12.4.2), as strictly as the compression layer does.
    fn parse(parameter: &str) -> Option<Self> {
        let value = parameter
            .strip_prefix("q=")
            .or_else(|| parameter.strip_prefix("Q="))?;
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        let mut thousandths = match whole {
            "0" => 0,
            "1" => 1000,
            _ => return None,
        };
        if fraction.len() > 3 {
            return None;
        }
        for (digit, scale) in fraction.bytes().zip([100, 10, 1]) {
            if !digit.is_ascii_digit() {
                return None;
            }
            thousandths += scale * u16::from(digit - b'0');
        }
        (thousandths <= 1000).then_some(Self(thousandths))
    }
}

/// The qualities a client listed for one coding.
#[derive(Clone, Copy, Debug, Default)]
struct Listed {
    first: Option<Quality>,
    highest: Option<Quality>,
}

impl Listed {
    fn record(&mut self, quality: Quality) {
        self.first = self.first.or(Some(quality));
        self.highest = self.highest.max(Some(quality));
    }
}

/// Each well-formed `coding;q=…` entry across every `Accept-Encoding`
/// header. An entry without a quality wants the coding fully.
fn preferences(request: &HeaderMap) -> impl Iterator<Item = (&str, Quality)> {
    request
        .get_all(ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|entry| match entry.split_once(';') {
            None => Some((entry.trim(), Quality::FULL)),
            Some((coding, parameter)) => Some((coding.trim(), Quality::parse(parameter.trim())?)),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn negotiate(values: &[&'static str]) -> ContentCoding {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(ACCEPT_ENCODING, HeaderValue::from_static(value));
        }
        ContentCoding::negotiate(&headers)
    }

    #[test]
    fn sends_gzip_to_clients_that_accept_it() {
        for value in [
            "gzip",
            "GZIP",
            "x-gzip",
            "deflate, gzip;q=1.0, *;q=0.5",
            "gzip;q=0.001",
            "gzip ; q=0.5",
            "identity, gzip",
            "identity;q=0.5, gzip",
            "identity;q=0, gzip",
            "*",
            "*;q=0, gzip",
            "*;q=0.5, gzip;q=1",
            "*;q=0.2, identity;q=0.1",
        ] {
            assert_eq!(negotiate(&[value]), ContentCoding::Gzip, "{value}");
        }
    }

    #[test]
    fn sends_identity_to_clients_that_do_not() {
        for value in [
            "",
            "deflate",
            "br, zstd",
            "gzip;q=0",
            "gzip;q=0.5, identity",
            "*;q=0, identity",
            "*;q=0",
            "identity;q=0",
            "gzip;q=0, *",
            "gzip;q=1.5",
            "gzip;q=0.0001",
            "gzip;q =1",
            "gzip;q=1;level=9",
            "**",
        ] {
            assert_eq!(negotiate(&[value]), ContentCoding::Identity, "{value}");
        }
        assert_eq!(
            ContentCoding::negotiate(&HeaderMap::new()),
            ContentCoding::Identity
        );
    }

    #[test]
    fn reads_every_accept_encoding_header() {
        assert_eq!(
            negotiate(&["identity;q=0.5", "gzip;q=0.8"]),
            ContentCoding::Gzip
        );
        assert_eq!(
            negotiate(&["gzip;q=0.5", "identity;q=0.8"]),
            ContentCoding::Identity
        );
    }

    #[test]
    fn without_a_wildcard_the_highest_listed_quality_counts() {
        assert_eq!(
            negotiate(&["gzip;q=0.1, identity;q=0.5, gzip;q=0.9"]),
            ContentCoding::Gzip
        );
    }

    #[test]
    fn with_a_wildcard_the_first_listed_quality_counts() {
        assert_eq!(
            negotiate(&["*, gzip;q=0.1, identity;q=0.5, gzip;q=0.9"]),
            ContentCoding::Identity
        );
    }

    #[test]
    fn parses_qualities_like_the_compression_layer() {
        for (parameter, thousandths) in [
            ("q=1", 1000),
            ("Q=0.5", 500),
            ("q=1.", 1000),
            ("q=1.000", 1000),
            ("q=0.125", 125),
        ] {
            assert_eq!(
                Quality::parse(parameter),
                Some(Quality(thousandths)),
                "{parameter}"
            );
        }
        for parameter in ["q=1.001", "q=2", "q=.5", "q=0.1234", "q= 1", "level=1"] {
            assert_eq!(Quality::parse(parameter), None, "{parameter}");
        }
    }

    #[test]
    fn both_codings_vary_on_accept_encoding() {
        let json = HeaderValue::from_static("application/json");

        let gzip = ContentCoding::Gzip.respond(json.clone(), Bytes::from_static(b"\x1f\x8b"));
        let identity = ContentCoding::Identity.respond(json, Bytes::from_static(b"{}"));

        assert_eq!(gzip.headers()[CONTENT_ENCODING], "gzip");
        assert_eq!(gzip.headers()[VARY], "accept-encoding");
        assert!(!identity.headers().contains_key(CONTENT_ENCODING));
        assert_eq!(identity.headers()[VARY], "accept-encoding");
        assert_eq!(identity.headers()[CONTENT_TYPE], "application/json");
    }
}
