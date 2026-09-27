use tokio::net::TcpListener;
use tracing::info;

use crate::{AppConfig, StartupError, build_app, infrastructure::millis};

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
        .await
        .map_err(StartupError::Serve)
}
