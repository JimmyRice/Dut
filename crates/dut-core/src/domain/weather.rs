//! Weather warnings issued by the Hong Kong Observatory.

use std::mem;

use jiff::Timestamp;

/// A warning or signal the Observatory has in force.
///
/// Every warning the Observatory documents is modelled, whether or not it
/// affects trains: deciding which ones matter is left to whoever reads them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WeatherWarning {
    TropicalCyclone(CycloneSignal),
    /// The pre-No. 8 special announcement: a No. 8 signal is expected within
    /// a few hours. It is announced alongside the signal in force.
    PreNo8Announcement,
    Rainstorm(RainstormLevel),
    FireDanger(FireDangerLevel),
    HotWeather,
    ColdWeather,
    Frost,
    StrongMonsoon,
    /// The special announcement on flooding in the northern New Territories.
    NorthernNtFlooding,
    Landslip,
    Tsunami,
    Thunderstorm,
    /// A warning this service does not recognise yet, kept as the Observatory
    /// coded it.
    Unrecognised(String),
}

/// A tropical cyclone warning signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CycloneSignal {
    One,
    Three,
    /// Gale or storm winds expected from the given quadrant.
    Eight(Quadrant),
    Nine,
    Ten,
}

/// The quadrant a No. 8 signal expects winds to come from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Quadrant {
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RainstormLevel {
    Amber,
    Red,
    Black,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FireDangerLevel {
    Yellow,
    Red,
}

impl WeatherWarning {
    /// Whether both are the same kind of warning, whatever their level.
    ///
    /// Levels of one kind replace each other: a No. 3 signal becomes a No. 8
    /// rather than both being in force.
    fn is_same_kind(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unrecognised(left), Self::Unrecognised(right)) => left == right,
            _ => mem::discriminant(self) == mem::discriminant(other),
        }
    }
}

/// A warning in force, with when the Observatory last touched it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveWarning {
    pub warning: WeatherWarning,
    /// When the Observatory last issued, reissued, extended, or updated it.
    pub updated_at: Option<Timestamp>,
}

/// Every warning the Observatory has in force.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WeatherWarnings {
    pub warnings: Vec<ActiveWarning>,
}

/// How one kind of warning differs between two readings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WarningChange {
    Issued(ActiveWarning),
    /// Still in force, at another level or with a newer update time, such as
    /// a No. 3 signal replaced by a No. 8.
    Changed {
        previous: ActiveWarning,
        current: ActiveWarning,
    },
    Cancelled(ActiveWarning),
}

impl WeatherWarnings {
    /// What changed since `previous`: warnings issued or changed, in the
    /// order they are listed now, then warnings cancelled.
    pub fn changes_since(&self, previous: &Self) -> Vec<WarningChange> {
        let issued_or_changed =
            self.warnings
                .iter()
                .filter_map(|current| match previous.same_kind_as(current) {
                    None => Some(WarningChange::Issued(current.clone())),
                    Some(before) if before != current => Some(WarningChange::Changed {
                        previous: before.clone(),
                        current: current.clone(),
                    }),
                    Some(_) => None,
                });
        let cancelled = previous
            .warnings
            .iter()
            .filter(|before| self.same_kind_as(before).is_none())
            .map(|before| WarningChange::Cancelled(before.clone()));

        issued_or_changed.chain(cancelled).collect()
    }

    fn same_kind_as(&self, other: &ActiveWarning) -> Option<&ActiveWarning> {
        self.warnings
            .iter()
            .find(|warning| warning.warning.is_same_kind(&other.warning))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> Option<Timestamp> {
        Some(Timestamp::from_second(seconds).expect("test timestamp should be in range"))
    }

    fn active(warning: WeatherWarning, updated_at: Option<Timestamp>) -> ActiveWarning {
        ActiveWarning {
            warning,
            updated_at,
        }
    }

    fn warnings(warnings: impl IntoIterator<Item = ActiveWarning>) -> WeatherWarnings {
        WeatherWarnings {
            warnings: warnings.into_iter().collect(),
        }
    }

    const NO_3: WeatherWarning = WeatherWarning::TropicalCyclone(CycloneSignal::Three);
    const NO_8_NE: WeatherWarning =
        WeatherWarning::TropicalCyclone(CycloneSignal::Eight(Quadrant::NorthEast));

    #[test]
    fn nothing_changes_between_identical_readings() {
        let reading = warnings([active(NO_3, at(0))]);

        assert!(reading.changes_since(&reading.clone()).is_empty());
    }

    #[test]
    fn a_stronger_signal_changes_the_cyclone_warning_in_place() {
        let before = warnings([active(NO_3, at(0))]);
        let after = warnings([active(NO_8_NE, at(60))]);

        assert_eq!(
            after.changes_since(&before),
            [WarningChange::Changed {
                previous: active(NO_3, at(0)),
                current: active(NO_8_NE, at(60)),
            }]
        );
    }

    #[test]
    fn the_pre_no_8_announcement_is_issued_alongside_the_signal() {
        let before = warnings([active(NO_3, at(0))]);
        let after = warnings([
            active(NO_3, at(0)),
            active(WeatherWarning::PreNo8Announcement, at(60)),
        ]);

        assert_eq!(
            after.changes_since(&before),
            [WarningChange::Issued(active(
                WeatherWarning::PreNo8Announcement,
                at(60)
            ))]
        );
    }

    #[test]
    fn a_reissued_warning_changes_with_its_update_time() {
        let before = warnings([active(WeatherWarning::HotWeather, at(0))]);
        let after = warnings([active(WeatherWarning::HotWeather, at(3_600))]);

        assert_eq!(
            after.changes_since(&before),
            [WarningChange::Changed {
                previous: active(WeatherWarning::HotWeather, at(0)),
                current: active(WeatherWarning::HotWeather, at(3_600)),
            }]
        );
    }

    #[test]
    fn a_warning_no_longer_listed_is_cancelled() {
        let amber = active(WeatherWarning::Rainstorm(RainstormLevel::Amber), at(0));
        let before = warnings([amber.clone(), active(NO_3, at(0))]);
        let after = warnings([active(NO_3, at(0))]);

        assert_eq!(
            after.changes_since(&before),
            [WarningChange::Cancelled(amber)]
        );
    }

    #[test]
    fn unrecognised_warnings_are_told_apart_by_their_code() {
        let before = warnings([active(
            WeatherWarning::Unrecognised("WNEW".to_owned()),
            at(0),
        )]);
        let after = warnings([active(
            WeatherWarning::Unrecognised("WOTHER".to_owned()),
            at(0),
        )]);

        let changes = after.changes_since(&before);

        assert!(matches!(
            changes.as_slice(),
            [WarningChange::Issued(_), WarningChange::Cancelled(_)]
        ));
    }
}
