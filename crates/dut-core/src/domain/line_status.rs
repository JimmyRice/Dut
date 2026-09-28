//! Service condition of each MTR line.

use jiff::Timestamp;

use crate::domain::network::Line;

/// What the MTR reports about a line's service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineCondition {
    /// Good service.
    Normal,
    /// Delays that need extra waiting or travel time. Also used while service
    /// gradually returns to normal.
    Delayed,
    /// Service disrupted; passengers are advised to use other transport.
    Disrupted,
    /// Delayed or disrupted. The MTR website renders this like `Delayed`.
    DelayedOrDisrupted,
    /// Outside service hours.
    NonServiceHours,
    /// A tropical cyclone warning signal is in force.
    TyphoonSignal,
    /// A condition code this service does not recognise yet, kept verbatim.
    Unknown(String),
}

/// The colour the MTR website uses to present a [`LineCondition`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayColor {
    Green,
    Yellow,
    Red,
    Grey,
    Typhoon,
}

impl LineCondition {
    /// How the MTR website presents this condition, so clients can match it.
    pub const fn display_color(&self) -> DisplayColor {
        match self {
            Self::Normal => DisplayColor::Green,
            Self::Delayed | Self::DelayedOrDisrupted => DisplayColor::Yellow,
            Self::Disrupted => DisplayColor::Red,
            Self::NonServiceHours | Self::Unknown(_) => DisplayColor::Grey,
            Self::TyphoonSignal => DisplayColor::Typhoon,
        }
    }
}

/// What the MTR reports about one line, with its explanation if any.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineStatus {
    pub line: Line,
    pub condition: LineCondition,
    pub message: Option<String>,
}

/// The condition of every line, as published by the MTR at `updated_at`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkStatus {
    pub updated_at: Timestamp,
    pub lines: Vec<LineStatus>,
}

/// A line whose condition or message differs between two readings.
///
/// Routine changes count: service ending for the night is a change from
/// [`LineCondition::Normal`] to [`LineCondition::NonServiceHours`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineStatusChange {
    pub previous: LineStatus,
    pub current: LineStatus,
}

impl NetworkStatus {
    /// The lines that changed since `previous`, in the order the feed lists
    /// them now. A line missing from either reading has nothing to compare
    /// with, so it is left out.
    pub fn changes_since(&self, previous: &Self) -> Vec<LineStatusChange> {
        self.lines
            .iter()
            .filter_map(|current| {
                let before = previous
                    .lines
                    .iter()
                    .find(|entry| entry.line == current.line)?;
                (before != current).then(|| LineStatusChange {
                    previous: before.clone(),
                    current: current.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_colours_follow_the_mtr_website() {
        let cases = [
            (LineCondition::Normal, DisplayColor::Green),
            (LineCondition::Delayed, DisplayColor::Yellow),
            (LineCondition::Disrupted, DisplayColor::Red),
            (LineCondition::DelayedOrDisrupted, DisplayColor::Yellow),
            (LineCondition::NonServiceHours, DisplayColor::Grey),
            (LineCondition::TyphoonSignal, DisplayColor::Typhoon),
            (
                LineCondition::Unknown("blue".to_owned()),
                DisplayColor::Grey,
            ),
        ];

        for (condition, expected) in cases {
            assert_eq!(condition.display_color(), expected, "{condition:?}");
        }
    }

    fn status(line: Line, condition: LineCondition, message: Option<&str>) -> LineStatus {
        LineStatus {
            line,
            condition,
            message: message.map(str::to_owned),
        }
    }

    fn network(lines: impl IntoIterator<Item = LineStatus>) -> NetworkStatus {
        NetworkStatus {
            updated_at: Timestamp::UNIX_EPOCH,
            lines: lines.into_iter().collect(),
        }
    }

    #[test]
    fn nothing_changes_between_identical_readings() {
        let reading = network([status(Line::KwunTong, LineCondition::Normal, None)]);

        assert!(reading.changes_since(&reading.clone()).is_empty());
    }

    #[test]
    fn the_end_of_service_is_a_change() {
        let before = network([
            status(Line::KwunTong, LineCondition::Normal, None),
            status(Line::Island, LineCondition::Normal, None),
        ]);
        let after = network([
            status(Line::KwunTong, LineCondition::NonServiceHours, None),
            status(Line::Island, LineCondition::Normal, None),
        ]);

        assert_eq!(
            after.changes_since(&before),
            [LineStatusChange {
                previous: status(Line::KwunTong, LineCondition::Normal, None),
                current: status(Line::KwunTong, LineCondition::NonServiceHours, None),
            }]
        );
    }

    #[test]
    fn a_new_message_is_a_change_even_in_the_same_condition() {
        let before = network([status(
            Line::KwunTong,
            LineCondition::Delayed,
            Some("Trains are delayed"),
        )]);
        let after = network([status(
            Line::KwunTong,
            LineCondition::Delayed,
            Some("Service is gradually resuming"),
        )]);

        let changes = after.changes_since(&before);

        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].current.message.as_deref(),
            Some("Service is gradually resuming")
        );
    }

    #[test]
    fn a_line_missing_from_the_earlier_reading_is_left_out() {
        let before = network([]);
        let after = network([status(Line::KwunTong, LineCondition::Delayed, None)]);

        assert!(after.changes_since(&before).is_empty());
    }

    #[test]
    fn a_new_publication_time_alone_is_not_a_change() {
        let before = network([status(Line::KwunTong, LineCondition::Normal, None)]);
        let after = NetworkStatus {
            updated_at: Timestamp::from_second(60).expect("test timestamp should be in range"),
            ..before.clone()
        };

        assert!(after.changes_since(&before).is_empty());
    }
}
