//! Port for upstream documents that are read in the background.

use std::future::Future;

use crate::{application::source::SourceUnavailable, domain::source_health::SourceId};

/// A document read whole from upstream, such as the MTR line status feed.
///
/// Every call fetches afresh. The caller decides how often to read the feed
/// and keeps the latest value itself, so implementations hold no cache.
///
/// # Examples
///
/// A feed that replays a recorded reading, as a test double does. A real
/// adapter in `dut-upstream` fetches the document through
/// `OutboundHttpClient`, converts its DTO into the domain type, and wraps any
/// failure in [`SourceUnavailable`]; `dut_poll::spawn` then reads it on a
/// schedule.
///
/// ```
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// use dut_core::{
///     application::{feed::Feed, source::SourceUnavailable},
///     domain::{line_status::NetworkStatus, source_health::SourceId},
/// };
/// use jiff::Timestamp;
///
/// /// The line status as recorded, or `None` while the MTR feed is down.
/// struct RecordedLineStatus(Option<NetworkStatus>);
///
/// impl Feed for RecordedLineStatus {
///     type Item = NetworkStatus;
///
///     const SOURCE: SourceId = SourceId::MtrLineStatus;
///
///     async fn fetch(&self) -> Result<NetworkStatus, SourceUnavailable> {
///         self.0
///             .clone()
///             .ok_or_else(|| SourceUnavailable::new("the MTR line status feed is down"))
///     }
/// }
///
/// let reading = NetworkStatus {
///     updated_at: Timestamp::UNIX_EPOCH,
///     lines: Vec::new(),
/// };
/// let up = RecordedLineStatus(Some(reading.clone()));
/// assert_eq!(up.fetch().await.ok(), Some(reading));
///
/// let down = RecordedLineStatus(None);
/// assert!(down.fetch().await.is_err());
/// # }
/// ```
pub trait Feed: Send + Sync + 'static {
    type Item: Send + Sync + 'static;

    /// Which source this feed reads, as named in logs and health reports.
    const SOURCE: SourceId;

    fn fetch(&self) -> impl Future<Output = Result<Self::Item, SourceUnavailable>> + Send;
}
