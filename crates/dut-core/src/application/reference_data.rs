//! Use case for MTR open data: the files as published and the datasets
//! cleaned from them.

use std::{fmt, future::Future, sync::Arc};

use crate::{
    application::source::{Snapshot, SourceUnavailable},
    domain::reference::ReferenceData,
};

/// Port for reading the latest MTR open data.
pub trait ReferenceDataSource: Send + Sync + 'static {
    fn reference_data(
        &self,
    ) -> impl Future<Output = Result<Snapshot<ReferenceData>, SourceUnavailable>> + Send;
}

/// Serves MTR open data to the API. Every dataset and file in a snapshot
/// comes from the same read, so a response assembled from several of them
/// is consistent.
pub struct ReferenceDataService<S> {
    source: Arc<S>,
}

impl<S: ReferenceDataSource> ReferenceDataService<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    pub async fn reference_data(&self) -> Result<Snapshot<ReferenceData>, SourceUnavailable> {
        self.source.reference_data().await
    }
}

// Implemented by hand so that neither requires the same of `S`.
impl<S> Clone for ReferenceDataService<S> {
    fn clone(&self) -> Self {
        Self {
            source: Arc::clone(&self.source),
        }
    }
}

impl<S> fmt::Debug for ReferenceDataService<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceDataService")
            .finish_non_exhaustive()
    }
}
