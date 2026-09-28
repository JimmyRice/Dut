//! Writes each event to the log with structured fields, so every change can
//! be traced whatever else subscribes to it.

use tracing::info;

use dut_core::{
    application::subscriber::Subscriber,
    domain::{
        event::{Change, MonitorEvent},
        line_status::LineStatusChange,
        next_train::{NextTrainSignal, SignalChange},
        weather::WarningChange,
    },
};

/// The subscriber the monitor always attaches, one log line per event.
#[derive(Debug)]
pub(crate) struct EventLog;

impl Subscriber for EventLog {
    const NAME: &'static str = "event_log";

    async fn on_event(&mut self, event: &MonitorEvent) {
        record(event);
    }
}

fn record(event: &MonitorEvent) {
    match &event.change {
        Change::LineStatus(change) => line_status(change),
        Change::WeatherWarning(change) => weather_warning(change),
        Change::NextTrainSignal(change) => next_train_signal(change),
        Change::SourceHealth(change) => info!(
            source = %change.source,
            previous = %change.previous,
            current = %change.current,
            "event: source health changed"
        ),
    }
}

fn line_status(change: &LineStatusChange) {
    let LineStatusChange { previous, current } = change;
    info!(
        line = %current.line,
        previous_condition = ?previous.condition,
        condition = ?current.condition,
        previous_message = previous.message.as_deref(),
        message = current.message.as_deref(),
        "event: line status changed"
    );
}

fn weather_warning(change: &WarningChange) {
    match change {
        WarningChange::Issued(warning) => info!(
            warning = ?warning.warning,
            updated_at = ?warning.updated_at,
            "event: weather warning issued"
        ),
        WarningChange::Changed { previous, current } => info!(
            previous = ?previous.warning,
            warning = ?current.warning,
            updated_at = ?current.updated_at,
            "event: weather warning changed"
        ),
        WarningChange::Cancelled(warning) => info!(
            warning = ?warning.warning,
            "event: weather warning cancelled"
        ),
    }
}

fn next_train_signal(change: &SignalChange) {
    info!(
        line = %change.line,
        station = %change.station,
        previous_delayed = change.previous.delayed,
        delayed = change.current.delayed,
        previous_notice = notice(&change.previous),
        notice = notice(&change.current),
        "event: next train signal changed"
    );
}

fn notice(signal: &NextTrainSignal) -> Option<&str> {
    signal
        .notice
        .as_ref()
        .map(|notice| notice.en.message.as_str())
}
