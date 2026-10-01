//! Response bodies encoded once per version and shared by every request for
//! that version.
//!
//! The open data responses are large, the MTR fare table alone being 1.7 MB
//! of JSON, but change at most daily. Serializing and gzipping one for each
//! request cost about 9 ms of CPU, and held 2 MB of memory until it was
//! sent; handing out the same encoded bytes costs neither.

use std::{
    error::Error,
    fmt,
    io::{self, Write},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use axum::{
    body::Bytes,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use flate2::{Compression, write::GzEncoder};
use thiserror::Error;
use tokio::{
    sync::OnceCell,
    task::{self, JoinError},
};
use tracing::{debug, error};

use dut_telemetry::millis;

use crate::content_coding::ContentCoding;

/// One response body in each content coding, for the version last asked
/// for.
///
/// A coding is encoded when a request first asks for it, and requests
/// arriving meanwhile wait for that encoding rather than start their own.
/// Encoding runs on Tokio's blocking pool, so a large body does not hold up
/// the other requests on its worker thread. Asking for another version
/// drops the previous version's encodings once no response still sends them.
pub(crate) struct EncodedBody<V> {
    /// Names the body in logs.
    name: &'static str,
    current: Mutex<Option<Arc<Encodings<V>>>>,
}

struct Encodings<V> {
    version: V,
    identity: OnceCell<Bytes>,
    gzip: OnceCell<Bytes>,
}

/// Why a body could not be encoded. None happens in practice: the bodies
/// serialize infallibly and compress into memory.
#[derive(Debug, Error)]
pub(crate) enum EncodeError {
    #[error("cannot serialize the response body")]
    Serialize(#[from] serde_json::Error),

    #[error("cannot gzip the response body")]
    Compress(#[from] io::Error),

    #[error("the encoding task did not finish")]
    Task(#[from] JoinError),
}

impl<V: Copy + PartialEq + fmt::Debug> EncodedBody<V> {
    pub(crate) const fn new(name: &'static str) -> Self {
        Self {
            name,
            current: Mutex::new(None),
        }
    }

    /// A `200` response with the body of `version` in the coding the
    /// request accepts.
    ///
    /// `render` produces the body unencoded. It runs at most once per
    /// version, since gzip starts from the unencoded body when a request has
    /// already asked for it, and not at all when every coding is cached.
    pub(crate) async fn respond(
        &self,
        request: &HeaderMap,
        version: V,
        content_type: HeaderValue,
        render: impl FnOnce() -> Result<Bytes, serde_json::Error> + Send + 'static,
    ) -> Response {
        let coding = ContentCoding::negotiate(request);
        match self.get(version, coding, render).await {
            Ok(body) => coding.respond(content_type, body),
            Err(err) => {
                error!(
                    body = self.name,
                    error = &err as &dyn Error,
                    "cannot encode the response body"
                );
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }

    /// The body of `version` in `coding`.
    async fn get(
        &self,
        version: V,
        coding: ContentCoding,
        render: impl FnOnce() -> Result<Bytes, serde_json::Error> + Send + 'static,
    ) -> Result<Bytes, EncodeError> {
        let encodings = self.encodings_of(version);
        match coding {
            ContentCoding::Identity => {
                self.encode(&encodings.identity, coding, move || Ok(render()?))
                    .await
            }
            ContentCoding::Gzip => {
                let identity = encodings.identity.get().cloned();
                self.encode(&encodings.gzip, coding, move || {
                    let plain = identity.map_or_else(render, Ok)?;
                    Ok(gzip(&plain)?)
                })
                .await
            }
        }
    }

    /// The encodings of `version`, replacing those of any other version.
    fn encodings_of(&self, version: V) -> Arc<Encodings<V>> {
        let mut current = self.current();
        if let Some(encodings) = current.as_ref().filter(|held| held.version == version) {
            return Arc::clone(encodings);
        }
        let encodings = Arc::new(Encodings::new(version));
        let previous = current.replace(Arc::clone(&encodings));
        drop(current);

        debug!(
            body = self.name,
            ?version,
            replaced = previous.is_some(),
            "new response body version"
        );
        encodings
    }

    /// The bytes in `cell`, from `work` on the blocking pool if no request
    /// has encoded them yet.
    async fn encode(
        &self,
        cell: &OnceCell<Bytes>,
        coding: ContentCoding,
        work: impl FnOnce() -> Result<Bytes, EncodeError> + Send + 'static,
    ) -> Result<Bytes, EncodeError> {
        if let Some(body) = cell.get() {
            return Ok(body.clone());
        }

        let mut encoded_here = false;
        let body = cell
            .get_or_try_init(|| async {
                encoded_here = true;
                let started = Instant::now();
                let body = task::spawn_blocking(work).await??;
                debug!(
                    body = self.name,
                    ?coding,
                    bytes = body.len(),
                    elapsed_ms = millis(started.elapsed()),
                    "encoded the response body"
                );
                Ok::<_, EncodeError>(body)
            })
            .await?;
        if !encoded_here {
            debug!(
                body = self.name,
                ?coding,
                "waited for the response body to be encoded"
            );
        }
        Ok(body.clone())
    }

    fn current(&self) -> MutexGuard<'_, Option<Arc<Encodings<V>>>> {
        // No code panics while holding this lock, so poisoning cannot leave
        // the slot half-updated; recover rather than propagate.
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// Implemented by hand so that it does not require `V: Debug` or the lock.
impl<V> fmt::Debug for EncodedBody<V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedBody")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<V> Encodings<V> {
    const fn new(version: V) -> Self {
        Self {
            version,
            identity: OnceCell::const_new(),
            gzip: OnceCell::const_new(),
        }
    }
}

/// Compresses at the highest level, which costs more only once per version
/// and makes every download of it smaller.
fn gzip(plain: &[u8]) -> io::Result<Bytes> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(plain)?;
    encoder.finish().map(Bytes::from)
}

#[cfg(test)]
mod tests {
    use std::{
        io::Read,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use flate2::read::GzDecoder;

    use super::*;

    const BODY: &[u8] = br#"{"fares":[1,2,3]}"#;

    /// A body that counts how often it is rendered.
    struct Counted {
        body: EncodedBody<u32>,
        renders: Arc<AtomicUsize>,
    }

    impl Counted {
        fn new() -> Self {
            Self {
                body: EncodedBody::new("test"),
                renders: Arc::default(),
            }
        }

        async fn get(&self, version: u32, coding: ContentCoding) -> Bytes {
            let renders = Arc::clone(&self.renders);
            self.body
                .get(version, coding, move || {
                    renders.fetch_add(1, Ordering::SeqCst);
                    Ok(Bytes::from_static(BODY))
                })
                .await
                .expect("the body should encode")
        }

        fn renders(&self) -> usize {
            self.renders.load(Ordering::SeqCst)
        }
    }

    fn gunzip(body: &[u8]) -> Vec<u8> {
        let mut plain = Vec::new();
        GzDecoder::new(body)
            .read_to_end(&mut plain)
            .expect("the body should be gzip");
        plain
    }

    #[tokio::test]
    async fn renders_each_version_once() {
        let counted = Counted::new();

        assert_eq!(counted.get(1, ContentCoding::Identity).await, BODY);
        assert_eq!(counted.get(1, ContentCoding::Identity).await, BODY);
        assert_eq!(counted.renders(), 1);

        counted.get(2, ContentCoding::Identity).await;
        assert_eq!(counted.renders(), 2);
    }

    #[tokio::test]
    async fn gzips_the_rendered_body() {
        let counted = Counted::new();

        let gzipped = counted.get(1, ContentCoding::Gzip).await;

        assert_eq!(gunzip(&gzipped), BODY);
        assert_eq!(counted.get(1, ContentCoding::Gzip).await, gzipped);
        assert_eq!(counted.renders(), 1);
    }

    #[tokio::test]
    async fn gzip_starts_from_an_unencoded_body_already_rendered() {
        let counted = Counted::new();

        counted.get(1, ContentCoding::Identity).await;
        counted.get(1, ContentCoding::Gzip).await;

        assert_eq!(counted.renders(), 1);
    }

    #[tokio::test]
    async fn requests_arriving_together_share_one_encoding() {
        let counted = Counted::new();

        let (first, second, third) = tokio::join!(
            counted.get(1, ContentCoding::Gzip),
            counted.get(1, ContentCoding::Gzip),
            counted.get(1, ContentCoding::Gzip),
        );

        assert_eq!(counted.renders(), 1);
        assert_eq!(first, second);
        assert_eq!(second, third);
    }

    #[tokio::test]
    async fn responds_in_the_negotiated_coding() {
        let body = EncodedBody::new("test");
        let mut request = HeaderMap::new();
        request.insert("accept-encoding", HeaderValue::from_static("gzip"));

        let response = body
            .respond(
                &request,
                1,
                HeaderValue::from_static("application/json"),
                || Ok(Bytes::from_static(BODY)),
            )
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-encoding"], "gzip");
        assert_eq!(response.headers()["content-type"], "application/json");
    }
}
