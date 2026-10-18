//! Accepts TCP connections and serves HTTP/1.1 on each, within limits that
//! keep a small machine answering when more clients arrive than it can
//! serve at once.

use std::{error::Error, future::Future, io, mem, pin::pin, sync::Arc, time::Duration};

use axum::Router;
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    server::graceful::GracefulShutdown,
    service::TowerToHyperService,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time,
};
use tracing::{debug, info, warn};

use dut_telemetry::millis;

/// How long a client may take to send a request's headers, counted from
/// when the server starts waiting for them, which includes the idle time
/// between requests on a kept-alive connection. A client that sends nothing,
/// or a byte at a time, is disconnected instead of holding a connection.
pub(super) const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to stop accepting after an error such as running out of file
/// descriptors, which would otherwise repeat at once in a busy loop.
const ACCEPT_ERROR_PAUSE: Duration = Duration::from_secs(1);

/// What one server process allows its clients.
#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    /// At most this many connections are open at once. Beyond that, new
    /// connections wait in the kernel's accept queue until one closes, so
    /// the memory the server holds stays bounded however many clients
    /// arrive, and the requests it has accepted are answered at full speed.
    pub max_connections: usize,
    /// See [`HEADER_READ_TIMEOUT`], which tests shorten.
    pub header_read_timeout: Duration,
}

/// Serves `router` on every connection `listener` accepts, within `limits`,
/// until `stop` resolves, then waits for open connections to finish their
/// requests.
pub(super) async fn serve(
    listener: TcpListener,
    router: Router,
    limits: Limits,
    stop: impl Future<Output = ()>,
) {
    let mut http = http1::Builder::new();
    // Hyper reads deadlines from the system clock, not Tokio's, so the
    // timeout runs in real time even under a paused test clock.
    http.timer(TokioTimer::new())
        .header_read_timeout(limits.header_read_timeout);
    let mut slots = Slots::new(limits.max_connections);
    let graceful = GracefulShutdown::new();
    let mut stop = pin!(stop);

    loop {
        let accepted = tokio::select! {
            accepted = accept(&listener, &mut slots) => accepted,
            () = stop.as_mut() => break,
        };
        let Some((stream, slot)) = accepted else {
            continue;
        };
        let connection = http.serve_connection(
            TokioIo::new(stream),
            TowerToHyperService::new(router.clone()),
        );
        let connection = graceful.watch(connection);
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                debug!(error = &err as &dyn Error, "connection ended with an error");
            }
            drop(slot);
        });
    }

    drop(listener);
    graceful.shutdown().await;
}

/// The next connection with the slot it holds, or `None` after an error
/// that is logged and paused on, if any.
async fn accept(
    listener: &TcpListener,
    slots: &mut Slots,
) -> Option<(TcpStream, OwnedSemaphorePermit)> {
    let slot = slots.take().await?;
    match listener.accept().await {
        Ok((stream, _)) => {
            // Responses are written whole, so delaying small packets to
            // coalesce them would only add latency.
            if let Err(err) = stream.set_nodelay(true) {
                debug!(
                    error = &err as &dyn Error,
                    "cannot disable Nagle's algorithm"
                );
            }
            Some((stream, slot))
        }
        Err(err) if is_connection_error(&err) => {
            debug!(
                error = &err as &dyn Error,
                "a connection failed before it was accepted"
            );
            None
        }
        Err(err) => {
            warn!(
                error = &err as &dyn Error,
                pause_ms = millis(ACCEPT_ERROR_PAUSE),
                "cannot accept connections; pausing"
            );
            time::sleep(ACCEPT_ERROR_PAUSE).await;
            None
        }
    }
}

/// The connections the server may still open, one permit each.
struct Slots {
    free: Arc<Semaphore>,
    max: usize,
    /// Whether every slot was taken when one was last asked for, so that a
    /// server held at its limit logs entering and leaving it once, rather
    /// than for each connection it accepts meanwhile.
    saturated: bool,
}

impl Slots {
    fn new(max: usize) -> Self {
        let max = max.min(Semaphore::MAX_PERMITS);
        Self {
            free: Arc::new(Semaphore::new(max)),
            max,
            saturated: false,
        }
    }

    /// A free slot, waiting for a connection to close when none is free.
    async fn take(&mut self) -> Option<OwnedSemaphorePermit> {
        if let Ok(slot) = Arc::clone(&self.free).try_acquire_owned() {
            if mem::take(&mut self.saturated) {
                info!(
                    max_connections = self.max,
                    "below the connection limit again"
                );
            }
            return Some(slot);
        }
        if !mem::replace(&mut self.saturated, true) {
            warn!(
                max_connections = self.max,
                "connection limit reached; new connections wait for one to close"
            );
        }
        // The semaphore is never closed, so this cannot fail in practice.
        Arc::clone(&self.free).acquire_owned().await.ok()
    }
}

/// Whether the error concerns only the connection being accepted, which the
/// client gave up on, rather than the listener.
fn is_connection_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests {
    use axum::routing::get;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::oneshot,
        task::JoinHandle,
    };

    use super::*;

    const REQUEST: &[u8] = b"GET / HTTP/1.1\r\nhost: test\r\n\r\n";

    /// Short, since Hyper's timeout runs on the real clock.
    const TIMEOUT: Duration = Duration::from_millis(300);

    /// A server on a free local port answering `ok`, which stops when the
    /// returned sender is dropped or sent to.
    async fn server(max_connections: usize) -> (u16, oneshot::Sender<()>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port should be free");
        let port = listener.local_addr().expect("bound").port();
        let router = Router::new().route("/", get(|| async { "ok" }));
        let (stop, stopped) = oneshot::channel::<()>();
        let limits = Limits {
            max_connections,
            header_read_timeout: TIMEOUT,
        };
        let serving = tokio::spawn(serve(listener, router, limits, async {
            let _ = stopped.await;
        }));
        (port, stop, serving)
    }

    async fn connect(port: u16) -> TcpStream {
        TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("the server should accept")
    }

    /// Sends one request and reads its response, keeping the connection.
    async fn request(stream: &mut TcpStream) -> String {
        stream.write_all(REQUEST).await.expect("request sent");
        let mut response = vec![0; 1024];
        let read = stream.read(&mut response).await.expect("response read");
        String::from_utf8_lossy(response.get(..read).unwrap_or_default()).into_owned()
    }

    /// Whether the server has closed `stream`, reading nothing more from it.
    async fn closed(stream: &mut TcpStream) -> bool {
        let mut byte = [0; 1];
        matches!(stream.read(&mut byte).await, Ok(0) | Err(_))
    }

    /// Whether `stream` is closed within a few timeouts, but not before one.
    async fn closed_after_timeout(stream: &mut TcpStream) -> bool {
        let started = time::Instant::now();
        let closed = time::timeout(TIMEOUT * 10, closed(stream)).await;
        matches!(closed, Ok(true)) && started.elapsed() >= TIMEOUT
    }

    #[tokio::test]
    async fn a_client_that_sends_no_headers_is_disconnected() {
        let (port, _stop, _serving) = server(4).await;
        let mut silent = connect(port).await;

        assert!(closed_after_timeout(&mut silent).await);
    }

    #[tokio::test]
    async fn an_idle_kept_alive_connection_is_closed_after_the_timeout() {
        let (port, _stop, _serving) = server(4).await;
        let mut client = connect(port).await;

        assert!(request(&mut client).await.starts_with("HTTP/1.1 200 OK"));
        assert!(closed_after_timeout(&mut client).await);
    }

    #[tokio::test]
    async fn connections_beyond_the_limit_wait_for_one_to_close() {
        let (port, _stop, _serving) = server(1).await;
        let mut first = connect(port).await;
        assert!(request(&mut first).await.starts_with("HTTP/1.1 200 OK"));

        let mut second = connect(port).await;
        second.write_all(REQUEST).await.expect("request sent");
        let mut response = [0; 64];
        let waiting = time::timeout(Duration::from_millis(200), second.read(&mut response)).await;
        assert!(waiting.is_err(), "the second connection was served early");

        drop(first);
        let read = second.read(&mut response).await.expect("response read");
        assert!(
            response
                .get(..read)
                .unwrap_or_default()
                .starts_with(b"HTTP/1.1 200 OK")
        );
    }

    #[tokio::test]
    async fn stopping_closes_idle_connections_and_returns() {
        let (port, stop, serving) = server(4).await;
        let mut client = connect(port).await;
        assert!(request(&mut client).await.starts_with("HTTP/1.1 200 OK"));

        stop.send(()).expect("the server is running");

        time::timeout(Duration::from_secs(5), serving)
            .await
            .expect("the server should stop")
            .expect("the server should not panic");
        assert!(closed(&mut client).await);
    }
}
