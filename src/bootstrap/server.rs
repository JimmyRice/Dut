use std::{error::Error, future};

use tokio::{net::TcpListener, signal};
use tracing::{info, warn};

use super::{AppConfig, StartupError, build_app};
use crate::telemetry::millis;

/// Serves the application on the configured address until the process is
/// asked to stop, then lets in-flight requests finish before returning.
pub async fn run(config: AppConfig) -> Result<(), StartupError> {
    let mtr = config.mtr();
    info!(
        bind_address = %config.bind_address(),
        outbound_http_timeout_ms = millis(config.outbound_http_timeout()),
        next_train_endpoint = mtr.next_train_endpoint,
        line_status_endpoint = mtr.line_status_endpoint,
        mtr_request_timeout_ms = millis(mtr.request_timeout),
        next_train_cache = ?mtr.next_train_cache,
        line_status_cache = ?mtr.line_status_cache,
        "starting server"
    );

    let app = build_app(&config)?;
    let bind_address = config.bind_address();
    let listener = TcpListener::bind(bind_address)
        .await
        .map_err(|source| StartupError::Bind {
            address: bind_address,
            source,
        })?;
    let local_address = listener.local_addr().map_err(StartupError::LocalAddress)?;
    info!("listening on http://{local_address}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_requested())
        .await
        .map_err(StartupError::Serve)
}

/// Resolves on Ctrl-C in a terminal, or on SIGTERM, which is how container
/// runtimes and service managers stop a process. As PID 1 in a container the
/// process is not killed by signals it does not handle, so without this
/// `docker stop` would wait out its timeout and then kill it.
async fn shutdown_requested() {
    let signal = tokio::select! {
        () = interrupt() => "SIGINT",
        () = terminate() => "SIGTERM",
    };
    info!(signal, "shutting down after in-flight requests finish");
}

/// Never resolves if the handler cannot be installed, so a missing signal
/// handler does not stop the server.
async fn interrupt() {
    if let Err(err) = signal::ctrl_c().await {
        warn!(error = &err as &dyn Error, "cannot listen for SIGINT");
        future::pending().await
    }
}

#[cfg(unix)]
async fn terminate() {
    match signal::unix::signal(signal::unix::SignalKind::terminate()) {
        Ok(mut signals) => {
            signals.recv().await;
        }
        Err(err) => {
            warn!(error = &err as &dyn Error, "cannot listen for SIGTERM");
            future::pending().await
        }
    }
}

#[cfg(not(unix))]
async fn terminate() {
    future::pending().await
}
