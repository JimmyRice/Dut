//! Time conventions of the MTR network.

use jiff::tz::{self, Offset};

/// Hong Kong Time (UTC+8).
///
/// Hong Kong has not observed daylight saving time since 1979, so a fixed
/// offset is exact and avoids depending on a time zone database at runtime.
///
/// # Examples
///
/// MTR feeds publish local times without an offset, and API responses show
/// times with it.
///
/// ```
/// use dut_core::domain::time::HONG_KONG;
/// use jiff::civil::date;
///
/// let last_train = HONG_KONG.to_timestamp(date(2026, 10, 2).at(0, 30, 0, 0))?;
/// assert_eq!(last_train.to_string(), "2026-10-01T16:30:00Z");
/// assert_eq!(
///     last_train.display_with_offset(HONG_KONG).to_string(),
///     "2026-10-02T00:30:00+08:00",
/// );
/// # Ok::<(), jiff::Error>(())
/// ```
pub const HONG_KONG: Offset = tz::offset(8);
