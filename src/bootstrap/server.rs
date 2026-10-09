use std::{
    error::Error,
    future::{self, IntoFuture},
    io,
    time::Duration,
};

use tokio::{net::TcpListener, signal, sync::oneshot, time};
use tracing::{info, warn};

use dut_telemetry::millis;

use super::{
    AppConfig, CommandLine, StartupError,
    app::{App, assemble},
};

/// Starts logging as the command line asks, then serves until the process
/// is asked to stop. Logging starts first, so that it records every step
/// of the start that follows.
pub async fn run(command_line: CommandLine) -> Result<(), StartupError> {
    dut_telemetry::init(&command_line.log_config())?;
    serve(command_line.app_config()).await
}

/// Serves the application on the configured address until the process is
/// asked to stop, then lets in-flight requests finish, for up to
/// [`SHUTDOWN_GRACE`], before returning.
///
/// Once the socket is bound, every upstream is probed once in the background
/// and the outcome is logged; requests are served meanwhile.
async fn serve(config: AppConfig) -> Result<(), StartupError> {
    info!(
        bind_address = %config.bind_address(),
        mock_api = config.mock_api(),
        outbound_http_timeout_ms = millis(config.outbound_http_timeout()),
        outbound_proxy = ?config.outbound_proxy(),
        "starting server"
    );
    log_upstreams(&config);

    let App {
        router,
        connectivity,
    } = assemble(&config)?;
    let bind_address = config.bind_address();
    let listener = TcpListener::bind(bind_address)
        .await
        .map_err(|source| StartupError::Bind {
            address: bind_address,
            source,
        })?;
    let local_address = listener.local_addr().map_err(StartupError::LocalAddress)?;
    info!("listening on http://{local_address}");
    tokio::spawn(connectivity.run());

    let (stopping, stop_requested) = oneshot::channel();
    let serving = axum::serve(listener, router).with_graceful_shutdown(async move {
        shutdown_requested().await;
        // The receiver outlives the server, so this cannot fail in practice.
        let _ = stopping.send(());
    });
    serve_until_stopped(serving, stop_requested, SHUTDOWN_GRACE)
        .await
        .map_err(StartupError::Serve)
}

/// Logs where each upstream is read and how long a request to it may take,
/// one line each, so a long endpoint does not bury the others. The caches
/// and pollers built on them log their own policies as they start.
fn log_upstreams(config: &AppConfig) {
    let (mtr, hko) = (config.mtr(), config.hko());
    for (upstream, endpoint, timeout) in [
        (
            "mtr.next_train",
            &mtr.next_train_endpoint,
            mtr.request_timeout,
        ),
        (
            "mtr.line_status",
            &mtr.line_status_endpoint,
            mtr.request_timeout,
        ),
        (
            "mtr.open_data",
            &mtr.open_data_endpoint,
            mtr.open_data_timeout,
        ),
        ("hko.warnings", &hko.warnings_endpoint, hko.request_timeout),
    ] {
        info!(upstream, endpoint = %endpoint, timeout_ms = millis(timeout), "upstream configured");
    }
}

/// How long in-flight requests may take to finish once a stop is requested.
/// An event stream never finishes by itself, and a container runtime kills a
/// process that outlasts its own timeout, ten seconds by default.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Runs `serving` until it ends, or until `grace` after `stop_requested`
/// resolves, whichever comes first. Graceful shutdown alone waits for every
/// open connection, so one client holding an event stream would keep the
/// process alive after Ctrl-C.
async fn serve_until_stopped(
    serving: impl IntoFuture<Output = io::Result<()>>,
    stop_requested: oneshot::Receiver<()>,
    grace: Duration,
) -> io::Result<()> {
    let grace_over = async {
        // A dropped sender means the server ended, which `serving` reports.
        if stop_requested.await.is_err() {
            future::pending::<()>().await;
        }
        time::sleep(grace).await;
    };
    tokio::select! {
        result = serving.into_future() => result,
        () = grace_over => {
            warn!(
                grace_ms = millis(grace),
                "closing connections still open after the grace period"
            );
            Ok(())
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_secs(5);

    #[tokio::test(start_paused = true)]
    async fn a_connection_that_never_ends_does_not_hold_the_process() {
        let (stopping, stop_requested) = oneshot::channel();
        let held_open = future::pending::<io::Result<()>>();
        let started = time::Instant::now();
        stopping.send(()).expect("the receiver is alive");

        let result = serve_until_stopped(held_open, stop_requested, GRACE).await;

        assert!(result.is_ok());
        assert_eq!(started.elapsed(), GRACE);
    }

    #[tokio::test(start_paused = true)]
    async fn requests_that_finish_within_the_grace_period_are_not_cut_short() {
        let (stopping, stop_requested) = oneshot::channel();
        let finishing = async {
            time::sleep(Duration::from_secs(2)).await;
            Ok(())
        };
        let started = time::Instant::now();
        stopping.send(()).expect("the receiver is alive");

        let result = serve_until_stopped(finishing, stop_requested, GRACE).await;

        assert!(result.is_ok());
        assert_eq!(started.elapsed(), Duration::from_secs(2));
    }

    #[tokio::test(start_paused = true)]
    async fn a_server_that_fails_before_any_stop_reports_its_error() {
        let (_stopping, stop_requested) = oneshot::channel();
        let failing = async { Err(io::Error::other("accept failed")) };

        let result = serve_until_stopped(failing, stop_requested, GRACE).await;

        assert!(result.is_err());
    }
}
