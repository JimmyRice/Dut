use std::{collections::BTreeSet, fmt::Display};

use tracing::warn;

use dut_core::domain::reference::SourceFile;

/// How many examples of skipped items a log line lists.
const EXAMPLES: usize = 5;

/// Rows left out of a dataset because they name something this service
/// cannot resolve, such as a station ID missing from the station list.
///
/// Such rows are expected to be rare and are not worth failing a whole
/// poll over, so they are counted and reported in one log line per reason.
#[derive(Debug)]
pub(super) struct Skipped {
    file: SourceFile,
    reason: &'static str,
    count: usize,
    examples: BTreeSet<String>,
}

impl Skipped {
    pub(super) const fn new(file: SourceFile, reason: &'static str) -> Self {
        Self {
            file,
            reason,
            count: 0,
            examples: BTreeSet::new(),
        }
    }

    /// Counts one skipped row, remembering what it named.
    pub(super) fn record(&mut self, item: impl Display) {
        self.count += 1;
        if self.examples.len() < EXAMPLES {
            self.examples.insert(item.to_string());
        }
    }

    /// Logs the count, if any rows were skipped.
    pub(super) fn report(&self) {
        if self.count > 0 {
            warn!(
                file = %self.file,
                reason = self.reason,
                skipped_rows = self.count,
                examples = ?self.examples,
                "left out open data rows that could not be resolved"
            );
        }
    }
}
