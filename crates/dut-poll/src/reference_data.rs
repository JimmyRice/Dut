//! Serves MTR open data from the polled feed, so the data endpoints read
//! what the poller already fetched instead of calling upstream themselves.

use dut_core::{
    application::{
        reference_data::ReferenceDataSource,
        source::{Snapshot, SourceUnavailable},
    },
    domain::reference::ReferenceData,
};

use crate::handle::FeedHandle;

impl ReferenceDataSource for FeedHandle<ReferenceData> {
    async fn reference_data(&self) -> Result<Snapshot<ReferenceData>, SourceUnavailable> {
        self.snapshot().await
    }
}
