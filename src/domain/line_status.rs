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
}
