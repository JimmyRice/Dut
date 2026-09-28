//! Conventions for log field values, so every crate records them alike.

use std::time::Duration;

/// A duration as whole milliseconds, the unit used in every log field.
pub fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
