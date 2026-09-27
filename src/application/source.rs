//! Vocabulary shared by application ports that read external data.

use std::{error::Error, sync::Arc, time::Duration};

use jiff::Timestamp;
use thiserror::Error;

/// A cached copy of upstream data, along with how trustworthy it still is.
///
/// The value sits behind an [`Arc`], so handing a snapshot to many concurrent
/// requests never copies the data itself.
#[derive(Debug)]
pub struct Snapshot<T> {
    value: Arc<T>,
    fetched_at: Timestamp,
    freshness: Freshness,
}

impl<T> Snapshot<T> {
    pub const fn new(value: Arc<T>, fetched_at: Timestamp, freshness: Freshness) -> Self {
        Self {
            value,
            fetched_at,
            freshness,
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    /// When this service received the data from upstream.
    pub const fn fetched_at(&self) -> Timestamp {
        self.fetched_at
    }

    pub const fn freshness(&self) -> Freshness {
        self.freshness
    }
}

// Implemented by hand so that cloning never requires `T: Clone`.
impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            fetched_at: self.fetched_at,
            freshness: self.freshness,
        }
    }
}

/// Whether a snapshot is still within the period it may be served as current.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    /// Current data that may be reused by downstream caches for `expires_in`.
    Fresh { expires_in: Duration },
    /// Past its freshness period, served because upstream could not provide
    /// newer data in time.
    Stale,
}

impl Freshness {
    pub const fn is_stale(self) -> bool {
        matches!(self, Self::Stale)
    }

    /// The freshness of a response assembled from several snapshots: it is
    /// only as fresh as its stalest part.
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Fresh { expires_in: left }, Self::Fresh { expires_in: right }) => Self::Fresh {
                expires_in: left.min(right),
            },
            _ => Self::Stale,
        }
    }
}

/// An upstream data source could not provide data, fresh or stale.
///
/// The underlying cause has already been logged where it happened; it is kept
/// as the error source for diagnostics and never shown to API clients.
#[derive(Debug, Error)]
#[error("the upstream data source is unavailable")]
pub struct SourceUnavailable(#[source] Box<dyn Error + Send + Sync>);

impl SourceUnavailable {
    pub fn new(cause: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(cause.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn fresh(seconds: u64) -> Freshness {
        Freshness::Fresh {
            expires_in: Duration::from_secs(seconds),
        }
    }

    #[test]
    fn combined_freshness_expires_with_the_earliest_part() {
        assert_eq!(fresh(10).combine(fresh(4)), fresh(4));
        assert_eq!(fresh(4).combine(fresh(10)), fresh(4));
    }

    #[test]
    fn combined_freshness_is_stale_if_any_part_is_stale() {
        assert_eq!(fresh(10).combine(Freshness::Stale), Freshness::Stale);
        assert_eq!(Freshness::Stale.combine(fresh(10)), Freshness::Stale);
    }
}
