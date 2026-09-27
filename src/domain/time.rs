//! Time conventions of the MTR network.

use jiff::tz::{self, Offset};

/// Hong Kong Time (UTC+8).
///
/// Hong Kong has not observed daylight saving time since 1979, so a fixed
/// offset is exact and avoids depending on a time zone database at runtime.
pub const HONG_KONG: Offset = tz::offset(8);
