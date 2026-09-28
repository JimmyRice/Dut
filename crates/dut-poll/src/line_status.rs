//! Serves the line status use case from the polled feed, so the endpoint
//! reads what the poller already fetched instead of calling upstream itself.

use dut_core::{
    application::{
        line_status::LineStatusSource,
        source::{Snapshot, SourceUnavailable},
    },
    domain::line_status::NetworkStatus,
};

use crate::handle::FeedHandle;

impl LineStatusSource for FeedHandle<NetworkStatus> {
    async fn status(&self) -> Result<Snapshot<NetworkStatus>, SourceUnavailable> {
        self.snapshot().await
    }
}
