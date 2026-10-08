//! Seeds, and the random streams derived from them.

use std::{
    fmt,
    hash::{BuildHasher, RandomState},
};

use jiff::Timestamp;

/// Picks one simulated world. The same seed simulates the same trains,
/// incidents, and messages, so a response can be reproduced, and boards
/// polled with one seed show their trains moving on with the clock.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Seed(u64);

impl Seed {
    /// The world a named scenario is simulated in when the client asks for
    /// no seed, so that boards polled one after another, or read at
    /// different stations, belong together.
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// A seed nobody asked for, so that a scenario drawn at random differs
    /// from one request to the next.
    pub fn fresh() -> Self {
        Self(RandomState::new().hash_one(Timestamp::now().as_nanosecond()))
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    /// A random stream for one purpose and set of keys, such as one train of
    /// one line. Streams for different purposes or keys are independent, so
    /// each choice stays the same however many others are made.
    pub(crate) fn stream(self, purpose: Purpose, keys: &[u64]) -> Rng {
        let state = keys
            .iter()
            .fold(mix(self.0 ^ purpose as u64), |state, key| mix(state ^ key));
        Rng(state)
    }
}

impl fmt::Display for Seed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// What a random stream decides. Each purpose has its own streams.
#[derive(Clone, Copy, Debug)]
#[repr(u64)]
pub(crate) enum Purpose {
    /// Which scenario `random` picks.
    Scenario = 0x5CE7_A210,
    /// Which line an incident affects, and how the MTR describes it.
    Incident = 0x1C1D_E472,
    /// Where in its cycle a line's timetable starts.
    Timetable = 0x7173_7AB1,
    /// How one train deviates from the timetable.
    Train = 0x7EA1_4000,
    /// How long after the MTR generated a board this service fetched it.
    Fetch = 0xFE7C_4000,
    /// When the last train of a direction runs.
    LastTrain = 0x1A57_7EA1,
}

/// SplitMix64: tiny, fast, and irregular enough for simulated timetables.
/// Never use it where an outcome must be unpredictable.
#[derive(Clone, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// Uniform in `0..bound`, or 0 when `bound` is 0.
    pub(crate) fn below(&mut self, bound: u64) -> u64 {
        // The high half of a 128-bit product spreads evenly without the
        // bias of `%`.
        ((u128::from(self.next_u64()) * u128::from(bound)) >> 64) as u64
    }

    /// Uniform in `low..=high`.
    pub(crate) fn between(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high.saturating_sub(low).saturating_add(1)).unwrap_or(0);
        low.saturating_add_unsigned(self.below(span))
    }

    /// Uniform in `0..len`, or 0 when `len` is 0: an index into a slice of
    /// `len` items.
    pub(crate) fn index(&mut self, len: usize) -> usize {
        let bound = u64::try_from(len).unwrap_or(u64::MAX);
        // A draw below `len` always fits back into `usize`.
        usize::try_from(self.below(bound)).unwrap_or(0)
    }

    /// True `percent` times in a hundred.
    pub(crate) fn percent(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    /// One of `items`, or `None` if there are none.
    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        items.get(self.index(items.len()))
    }
}

/// A key for a code such as `TKL` or `TKO`: its bytes, which are unique for
/// codes of up to eight letters.
pub(crate) fn text_key(text: &str) -> u64 {
    text.bytes().fold(0, |key, byte| key << 8 | u64::from(byte))
}

/// SplitMix64's finaliser, which spreads every input bit over the output.
const fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draws(mut rng: Rng) -> Vec<u64> {
        (0..4).map(|_| rng.next_u64()).collect()
    }

    #[test]
    fn a_stream_repeats_for_the_same_seed_purpose_and_keys() {
        let seed = Seed::new(42);

        assert_eq!(
            draws(seed.stream(Purpose::Train, &[1, 2])),
            draws(seed.stream(Purpose::Train, &[1, 2]))
        );
    }

    #[test]
    fn streams_differ_by_seed_purpose_and_keys() {
        let base = draws(Seed::new(42).stream(Purpose::Train, &[1, 2]));

        assert_ne!(base, draws(Seed::new(43).stream(Purpose::Train, &[1, 2])));
        assert_ne!(base, draws(Seed::new(42).stream(Purpose::Fetch, &[1, 2])));
        assert_ne!(base, draws(Seed::new(42).stream(Purpose::Train, &[2, 1])));
    }

    #[test]
    fn bounded_draws_stay_in_range() {
        let mut rng = Seed::DEFAULT.stream(Purpose::Train, &[]);

        for _ in 0..1_000 {
            assert!(rng.below(7) < 7);
            assert!((-3..=3).contains(&rng.between(-3, 3)));
        }
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.between(5, 5), 5);
        assert_eq!(rng.pick::<u8>(&[]), None);
    }

    #[test]
    fn bounded_draws_reach_every_value() {
        let mut rng = Seed::new(7).stream(Purpose::Train, &[]);
        let mut seen = [false; 5];

        for _ in 0..200 {
            seen[rng.index(5)] = true;
        }

        assert_eq!(seen, [true; 5]);
    }

    #[test]
    fn codes_have_distinct_keys() {
        assert_ne!(text_key("TKL"), text_key("TKO"));
        assert_ne!(text_key("LR"), text_key("LRT"));
    }
}
