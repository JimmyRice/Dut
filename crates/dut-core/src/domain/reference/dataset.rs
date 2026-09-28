use std::{
    fmt,
    hash::{Hash, Hasher},
};

use jiff::Timestamp;

/// One version of a dataset or file. Equal content has an equal revision,
/// so clients compare revisions to decide whether to download again.
///
/// It is an FNV-1a hash rather than one from `std`'s randomly keyed
/// hasher, so the same content keeps its revision across restarts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Revision(u64);

impl Revision {
    /// The revision of a value, through its `Hash` implementation.
    pub fn of(value: &impl Hash) -> Self {
        let mut hasher = Fnv1a::new();
        value.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// The revision of raw bytes, such as a file as upstream published it.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Fnv1a::new();
        hasher.write(bytes);
        Self(hasher.finish())
    }
}

/// Sixteen lowercase hex digits, such as `9f86d081884c7d65`.
impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

/// A dataset cleaned from MTR open data, with its revision and when the MTR
/// last changed the files behind it.
pub struct Dataset<T> {
    value: T,
    revision: Revision,
    updated_at: Option<Timestamp>,
}

impl<T: Hash> Dataset<T> {
    /// The revision is derived from the cleaned value, so it changes exactly
    /// when what clients receive changes, whatever changed upstream.
    pub fn new(value: T, updated_at: Option<Timestamp>) -> Self {
        Self {
            revision: Revision::of(&value),
            value,
            updated_at,
        }
    }
}

impl<T> Dataset<T> {
    pub const fn value(&self) -> &T {
        &self.value
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// When the MTR last changed the files the dataset is built from, from
    /// their `Last-Modified` headers; `None` when upstream did not say.
    pub const fn updated_at(&self) -> Option<Timestamp> {
        self.updated_at
    }
}

// Implemented by hand so that it neither requires `T: Debug` nor prints a
// whole fare table into a log line.
impl<T> fmt::Debug for Dataset<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Dataset")
            .field("revision", &self.revision)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

/// The 64-bit FNV-1a hash: tiny, deterministic, and good enough to tell
/// versions of a dataset apart. It is not collision-resistant against
/// deliberate tampering, which upstream data does not call for.
struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;

    const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }
}

impl Hasher for Fnv1a {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv1a_test_vectors() {
        assert_eq!(Revision::of_bytes(b"").to_string(), "cbf29ce484222325");
        assert_eq!(Revision::of_bytes(b"a").to_string(), "af63dc4c8601ec8c");
    }

    #[test]
    fn equal_values_share_a_revision_and_different_ones_do_not() {
        let fares = vec![("CEN", "ADM", 490_u32)];

        assert_eq!(Revision::of(&fares), Revision::of(&fares.clone()));
        assert_ne!(
            Revision::of(&fares),
            Revision::of(&vec![("CEN", "ADM", 500_u32)])
        );
    }

    #[test]
    fn a_dataset_takes_its_revision_from_its_value() {
        let dataset = Dataset::new(vec![1_u8, 2, 3], None);

        assert_eq!(dataset.revision(), Revision::of(&vec![1_u8, 2, 3]));
        assert_eq!(dataset.value(), &[1, 2, 3]);
    }
}
