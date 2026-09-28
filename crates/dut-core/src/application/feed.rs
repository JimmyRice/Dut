//! Port for upstream documents that are read in the background.

use std::future::Future;

use crate::{application::source::SourceUnavailable, domain::source_health::SourceId};

/// A document read whole from upstream, such as the MTR line status feed.
///
/// Every call fetches afresh. The caller decides how often to read the feed
/// and keeps the latest value itself, so implementations hold no cache.
pub trait Feed: Send + Sync + 'static {
    type Item: Send + Sync + 'static;

    /// Which source this feed reads, as named in logs and health reports.
    const SOURCE: SourceId;

    fn fetch(&self) -> impl Future<Output = Result<Self::Item, SourceUnavailable>> + Send;
}
