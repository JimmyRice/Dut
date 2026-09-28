use std::{fmt, str::FromStr};

use thiserror::Error;

/// An amount in Hong Kong cents: HK$4.90 is `490`.
///
/// The MTR publishes fares in dollars with at most two decimal places, so
/// whole cents hold every fare exactly, and clients never round a float.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fare(u32);

impl Fare {
    pub const fn from_cents(cents: u32) -> Self {
        Self(cents)
    }

    pub const fn cents(self) -> u32 {
        self.0
    }
}

/// Parses a dollar amount as the MTR publishes it, such as `4.90`, `52.5`,
/// or `120`.
impl FromStr for Fare {
    type Err = InvalidFare;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (dollars, decimals) = match input.split_once('.') {
            Some((dollars, decimals)) if !decimals.is_empty() => (dollars, decimals),
            Some(_) => return Err(InvalidFare),
            None => (input, ""),
        };
        if dollars.is_empty() || !dollars.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidFare);
        }
        // `4.9` and `4.90` are both 490 cents.
        let digit = |byte: &u8| byte.is_ascii_digit().then(|| u32::from(byte - b'0'));
        let cents = match decimals.as_bytes() {
            [] => Some(0),
            [tens] => digit(tens).map(|tens| tens * 10),
            [tens, ones] => digit(tens)
                .zip(digit(ones))
                .map(|(tens, ones)| tens * 10 + ones),
            _ => None,
        }
        .ok_or(InvalidFare)?;
        dollars
            .parse::<u32>()
            .ok()
            .and_then(|dollars| dollars.checked_mul(100))
            .and_then(|amount| amount.checked_add(cents))
            .map(Self)
            .ok_or(InvalidFare)
    }
}

/// `HK$4.90`.
impl fmt::Display for Fare {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "HK${}.{:02}", self.0 / 100, self.0 % 100)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a fare must be a dollar amount with at most two decimal places")]
pub struct InvalidFare;

/// Fares on the MTR's heavy rail lines and on Light Rail, which sell the
/// same kinds of ticket.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RailFares {
    pub octopus: OctopusFares,
    pub single_journey: SingleJourneyFares,
}

/// Fares charged to an Octopus card, by who holds it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OctopusFares {
    pub adult: Fare,
    /// A Student Octopus under the Student Travel Scheme.
    pub student: Fare,
    /// A JoyYou Card, for residents aged 60 to 64.
    pub joyyou_sixty: Fare,
    pub child: Fare,
    /// Residents aged 65 and over.
    pub elderly: Fare,
    /// Persons with disabilities.
    pub disability: Fare,
}

/// Fares for a single journey ticket, which offers fewer concessions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SingleJourneyFares {
    pub adult: Fare,
    pub child: Fare,
    pub elderly: Fare,
}

/// Airport Express fares, which only tell adults from children.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AirportExpressFares {
    pub octopus: AdultAndChildFares,
    pub single_journey: AdultAndChildFares,
}

/// The two fares of an Airport Express ticket.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AdultAndChildFares {
    pub adult: Fare,
    pub child: Fare,
}

/// One trip between two stops, and what it costs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Trip<K, F> {
    pub from: K,
    pub to: K,
    pub fares: F,
}

/// The fares of every trip on a network, keyed by origin and destination:
/// station codes for the MTR, stop numbers for Light Rail.
#[derive(Eq, Hash, PartialEq)]
pub struct FareTable<K, F> {
    /// Sorted by origin, then destination.
    trips: Vec<Trip<K, F>>,
}

impl<K: Copy + Ord, F> FareTable<K, F> {
    /// Collects trips in any order. A trip that starts and ends at the same
    /// stop has no fare worth showing and is left out; when a trip appears
    /// twice, the first wins.
    pub fn new(trips: impl IntoIterator<Item = Trip<K, F>>) -> Self {
        let mut trips: Vec<_> = trips
            .into_iter()
            .filter(|trip| trip.from != trip.to)
            .collect();
        trips.sort_by_key(|trip| (trip.from, trip.to));
        trips.dedup_by_key(|trip| (trip.from, trip.to));
        Self { trips }
    }

    /// The fares from `from` to `to`, if the table has that trip.
    pub fn get(&self, from: K, to: K) -> Option<&F> {
        self.trips
            .binary_search_by_key(&(from, to), |trip| (trip.from, trip.to))
            .ok()
            .and_then(|index| self.trips.get(index))
            .map(|trip| &trip.fares)
    }
}

impl<K, F> FareTable<K, F> {
    /// Every trip, ordered by origin, then destination.
    pub fn trips(&self) -> &[Trip<K, F>] {
        &self.trips
    }
}

// Implemented by hand so that it neither requires `K: Debug` nor prints
// thousands of trips into a log line.
impl<K, F> fmt::Debug for FareTable<K, F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FareTable")
            .field("trips", &self.trips.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fare(input: &str) -> Result<u32, InvalidFare> {
        input.parse::<Fare>().map(Fare::cents)
    }

    #[test]
    fn parses_published_dollar_amounts_into_cents() {
        assert_eq!(fare("4.90"), Ok(490));
        assert_eq!(fare("52.5"), Ok(5250));
        assert_eq!(fare("120"), Ok(12_000));
        assert_eq!(fare("0.00"), Ok(0));
        assert_eq!(fare("3.05"), Ok(305));
    }

    #[test]
    fn rejects_amounts_that_are_not_whole_cents() {
        for input in [
            "", ".", ".5", "4.", "4.905", "-4.90", "4,90", "HK$4.90", "4.9x",
        ] {
            assert_eq!(fare(input), Err(InvalidFare), "{input:?}");
        }
    }

    #[test]
    fn displays_as_dollars() {
        assert_eq!(Fare::from_cents(490).to_string(), "HK$4.90");
        assert_eq!(Fare::from_cents(12_000).to_string(), "HK$120.00");
    }

    fn trip(from: u16, to: u16, cents: u32) -> Trip<u16, Fare> {
        Trip {
            from,
            to,
            fares: Fare::from_cents(cents),
        }
    }

    #[test]
    fn looks_up_trips_in_either_direction() {
        let table = FareTable::new([trip(2, 1, 510), trip(1, 2, 500)]);

        assert_eq!(table.get(1, 2), Some(&Fare::from_cents(500)));
        assert_eq!(table.get(2, 1), Some(&Fare::from_cents(510)));
        assert_eq!(table.get(1, 3), None);
    }

    #[test]
    fn leaves_out_trips_to_the_same_stop_and_repeats() {
        let table = FareTable::new([trip(1, 1, 0), trip(1, 2, 500), trip(1, 2, 999)]);

        assert_eq!(table.trips(), &[trip(1, 2, 500)]);
    }
}
