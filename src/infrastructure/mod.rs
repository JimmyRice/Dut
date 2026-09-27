use std::time::Duration;

pub mod cache;
pub mod http_client;
pub mod http_freshness;
pub mod mtr;

/// A duration as whole milliseconds, the unit used in every log field.
pub(crate) fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
