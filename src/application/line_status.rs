//! Use cases for the service condition of each line.

use std::{fmt, future::Future, sync::Arc};

use crate::{
    application::source::{Snapshot, SourceUnavailable},
    domain::line_status::NetworkStatus,
};

/// Port for reading the condition of every line.
pub trait LineStatusSource: Send + Sync + 'static {
    fn status(
        &self,
    ) -> impl Future<Output = Result<Snapshot<NetworkStatus>, SourceUnavailable>> + Send;
}

pub struct LineStatusService<S> {
    source: Arc<S>,
}

impl<S: LineStatusSource> LineStatusService<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    pub async fn status(&self) -> Result<Snapshot<NetworkStatus>, SourceUnavailable> {
        self.source.status().await
    }
}

// Implemented by hand so that neither requires the same of `S`.
impl<S> Clone for LineStatusService<S> {
    fn clone(&self) -> Self {
        Self {
            source: Arc::clone(&self.source),
        }
    }
}

impl<S> fmt::Debug for LineStatusService<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LineStatusService")
            .finish_non_exhaustive()
    }
}
