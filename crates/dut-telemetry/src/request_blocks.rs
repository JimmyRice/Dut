//! Console view that prints each HTTP request's log events as one block.

use std::{
    error::Error,
    fmt::{self, Write as _},
    io::Write as _,
    iter,
};

use jiff::Zoned;
use nu_ansi_term::{Color, Style};
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{
    fmt::MakeWriter,
    layer::{Context, Layer},
    registry::{LookupSpan, SpanRef},
};

/// Name of the span that wraps one inbound request.
///
/// Events inside it are held back and printed together once it closes, which
/// is after the response body has been sent and any background refresh the
/// request started has finished. The block header reads the span's `method`,
/// `path`, `id`, `status`, and `latency_ms` fields.
pub const REQUEST_SPAN: &str = "request";

/// Request start times carry the date and offset; event times within a block
/// only need to order events and measure gaps between them.
const FULL_TIME: &str = "%Y-%m-%d %H:%M:%S%.3f %:z";
const SHORT_TIME: &str = "%H:%M:%S%.3f";

/// A [`Layer`] that renders events for people watching a terminal.
///
/// Concurrent requests would otherwise interleave line by line; buffering per
/// request keeps each one readable top to bottom, at the cost of printing it
/// only when it completes.
pub(super) struct RequestBlocks<W> {
    make_writer: W,
    palette: Palette,
    clock: fn() -> Zoned,
}

impl<W> RequestBlocks<W> {
    /// `colour` should be off unless the writer is a terminal that wants it.
    pub(super) fn new(make_writer: W, colour: bool) -> Self {
        Self {
            make_writer,
            palette: Palette::new(colour),
            clock: Zoned::now,
        }
    }
}

impl<S, W> Layer<S> for RequestBlocks<W>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + 'static,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut fields = Fields::default();
        attrs.record(&mut fields);

        let mut extensions = span.extensions_mut();
        extensions.insert(fields);
        if span.name() == REQUEST_SPAN {
            extensions.insert(Block {
                opened_at: (self.clock)(),
                entries: String::new(),
            });
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if let Some(fields) = span.extensions_mut().get_mut::<Fields>() {
            values.record(fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let now = (self.clock)();
        let scope: Vec<_> = ctx
            .event_scope(event)
            .map(|scope| scope.from_root().collect())
            .unwrap_or_default();
        match scope.iter().rposition(|span| span.name() == REQUEST_SPAN) {
            Some(index) => self.buffer(event, &now, &scope[index], &scope[index + 1..]),
            None => self.print(event, &now, &scope),
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let rendered = {
            let mut extensions = span.extensions_mut();
            let Some(block) = extensions.remove::<Block>() else {
                return;
            };
            let fields = extensions.get_mut::<Fields>();
            let mut rendered = String::new();
            if self
                .write_block(&mut rendered, &block, fields.as_deref())
                .is_err()
            {
                return;
            }
            rendered
        };
        self.emit(&rendered);
    }
}

impl<W> RequestBlocks<W>
where
    W: for<'w> MakeWriter<'w>,
{
    /// Adds the event to its request's block, to be printed when it closes.
    fn buffer<S>(
        &self,
        event: &Event<'_>,
        now: &Zoned,
        request: &SpanRef<'_, S>,
        context: &[SpanRef<'_, S>],
    ) where
        S: for<'a> LookupSpan<'a>,
    {
        let mut entry = String::new();
        let gutter = &self.palette.gutter;
        if self
            .write_event(&mut entry, gutter, now, SHORT_TIME, event, context)
            .is_err()
        {
            return;
        }
        if let Some(block) = request.extensions_mut().get_mut::<Block>() {
            block.entries.push_str(&entry);
        }
    }

    /// Prints an event that belongs to no request, such as startup.
    fn print<S>(&self, event: &Event<'_>, now: &Zoned, context: &[SpanRef<'_, S>])
    where
        S: for<'a> LookupSpan<'a>,
    {
        let mut entry = String::new();
        if self
            .write_event(&mut entry, "", now, FULL_TIME, event, context)
            .is_ok()
        {
            entry.push('\n');
            self.emit(&entry);
        }
    }

    /// Writes a whole entry in one call, so it cannot interleave with others.
    fn emit(&self, text: &str) {
        // A console that cannot be written to has nowhere to report it.
        let _ = self.make_writer.make_writer().write_all(text.as_bytes());
    }

    fn write_block(&self, out: &mut String, block: &Block, fields: Option<&Fields>) -> fmt::Result {
        let palette = &self.palette;
        let field = |name| fields.and_then(|fields| fields.get(name));
        let method = field("method").unwrap_or("-");
        let path = field("path").unwrap_or("-");

        paint(out, palette.dimmed, format_args!("┌─ "))?;
        paint(out, palette.bold, format_args!("{method} {path}"))?;
        out.push_str(" → ");
        match field("status") {
            Some(status) => {
                paint(out, palette.status(status), format_args!("{status}"))?;
                if let Some(latency) = field("latency_ms") {
                    write!(out, " in {latency} ms")?;
                }
            }
            None => paint(out, palette.dimmed, format_args!("no response"))?,
        }
        out.push('\n');

        out.push_str(&palette.gutter);
        paint(
            out,
            palette.dimmed,
            format_args!("{}", block.opened_at.strftime(FULL_TIME)),
        )?;
        if let Some(id) = field("id") {
            paint(out, palette.dimmed, format_args!("  id {id}"))?;
        }
        out.push('\n');
        paint(out, palette.dimmed, format_args!("│\n"))?;

        out.push_str(&block.entries);
        paint(out, palette.dimmed, format_args!("└─\n\n"))
    }

    /// Writes the event on one line, then one line per enclosing span below
    /// the request, outermost first, so concurrent work inside a request
    /// (such as one upstream call per line at a station) can be told apart.
    fn write_event<S>(
        &self,
        out: &mut String,
        gutter: &str,
        time: &Zoned,
        time_format: &str,
        event: &Event<'_>,
        context: &[SpanRef<'_, S>],
    ) -> fmt::Result
    where
        S: for<'a> LookupSpan<'a>,
    {
        let palette = &self.palette;
        let metadata = event.metadata();
        let mut fields = Fields::default();
        event.record(&mut fields);
        let message = fields.take("message");
        let target = fields.take_log_target();
        let stamp = time.strftime(time_format).to_string();

        out.push_str(gutter);
        paint(out, palette.dimmed, format_args!("{stamp} "))?;
        let level = metadata.level();
        paint(
            out,
            palette.level(level),
            format_args!("{:>5}", level.as_str()),
        )?;
        out.push(' ');
        let target = target.as_deref().unwrap_or(metadata.target());
        paint(out, palette.dimmed, format_args!("{target}:"))?;
        if let Some(message) = message {
            write!(out, " {message}")?;
        }
        palette.write_fields(out, &fields)?;
        out.push('\n');

        let indent = stamp.chars().count() + " LEVEL ".len();
        for span in context {
            write!(out, "{gutter}{:indent$}", "")?;
            paint(out, palette.dimmed, format_args!("↳ "))?;
            paint(out, palette.bold, format_args!("{}", span.name()))?;
            if let Some(fields) = span.extensions().get::<Fields>() {
                palette.write_fields(out, fields)?;
            }
            out.push('\n');
        }
        Ok(())
    }
}

/// The events of one request, waiting for the request to finish.
struct Block {
    opened_at: Zoned,
    entries: String,
}

/// Field values rendered as text, in the order they were recorded.
#[derive(Default)]
struct Fields(Vec<(&'static str, String)>);

impl Fields {
    fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(field, _)| *field == name)
            .map(|(_, value)| value.as_str())
    }

    fn take(&mut self, name: &str) -> Option<String> {
        let index = self.0.iter().position(|(field, _)| *field == name)?;
        Some(self.0.remove(index).1)
    }

    fn set(&mut self, name: &'static str, value: String) {
        match self.0.iter_mut().find(|(field, _)| *field == name) {
            Some((_, existing)) => *existing = value,
            None => self.0.push((name, value)),
        }
    }

    /// Records bridged from the `log` crate arrive with their origin in
    /// `log.*` fields; use it as the target and hide the rest.
    fn take_log_target(&mut self) -> Option<String> {
        let target = self.take("log.target");
        self.0.retain(|(field, _)| !field.starts_with("log."));
        target
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();
        if name == "message" || name.starts_with("log.") {
            self.set(name, value.to_owned());
        } else {
            self.record_debug(field, &value);
        }
    }

    /// Includes the source chain, as `tracing`'s own formatter does, so the
    /// root cause of an upstream failure is visible.
    fn record_error(&mut self, field: &Field, value: &(dyn Error + 'static)) {
        let mut text = value.to_string();
        let mut sources = iter::successors(value.source(), |&error| error.source()).peekable();
        if sources.peek().is_some() {
            let sources: Vec<_> = sources.map(ToString::to_string).collect();
            let _ = write!(text, " {}.sources=[{}]", field.name(), sources.join(", "));
        }
        self.set(field.name(), text);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.set(field.name(), format!("{value:?}"));
    }
}

/// Terminal styles, all plain when colour is off.
struct Palette {
    colour: bool,
    dimmed: Style,
    bold: Style,
    italic: Style,
    /// Left border of a request block, before each of its lines.
    gutter: String,
}

impl Palette {
    fn new(colour: bool) -> Self {
        let style = |style: Style| if colour { style } else { Style::new() };
        let dimmed = style(Style::new().dimmed());
        Self {
            colour,
            dimmed,
            bold: style(Style::new().bold()),
            italic: style(Style::new().italic()),
            gutter: format!("{}  ", dimmed.paint("│")),
        }
    }

    /// The same colours `tracing`'s own formatter uses.
    fn level(&self, level: &Level) -> Style {
        if !self.colour {
            return Style::new();
        }
        match *level {
            Level::TRACE => Color::Purple.normal(),
            Level::DEBUG => Color::Blue.normal(),
            Level::INFO => Color::Green.normal(),
            Level::WARN => Color::Yellow.normal(),
            Level::ERROR => Color::Red.normal(),
        }
    }

    fn status(&self, status: &str) -> Style {
        if !self.colour {
            return Style::new();
        }
        match status.parse::<u16>() {
            Ok(200..=399) => Color::Green.bold(),
            Ok(400..=499) => Color::Yellow.bold(),
            _ => Color::Red.bold(),
        }
    }

    fn write_fields(&self, out: &mut String, fields: &Fields) -> fmt::Result {
        for (name, value) in &fields.0 {
            out.push(' ');
            paint(out, self.italic, format_args!("{name}"))?;
            paint(out, self.dimmed, format_args!("="))?;
            out.push_str(value);
        }
        Ok(())
    }
}

/// Writes formatted text in a style without allocating a string for it.
fn paint(out: &mut String, style: Style, text: fmt::Arguments<'_>) -> fmt::Result {
    write!(out, "{}{text}{}", style.prefix(), style.suffix())
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use tracing::{debug, info, info_span, warn};
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    fn fixed_clock() -> Zoned {
        "2026-09-28T01:17:44.538355+08:00[+08:00]".parse().unwrap()
    }

    /// Collects everything the layer prints.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    /// Runs `emit` against a colourless layer and returns what it printed.
    fn render(emit: impl FnOnce(&dyn Fn() -> String)) -> String {
        let captured = Captured::default();
        let writer = captured.clone();
        let layer = RequestBlocks {
            clock: fixed_clock,
            ..RequestBlocks::new(move || writer.clone(), false)
        };
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            emit(&|| captured.text());
        });
        captured.text()
    }

    #[test]
    fn holds_back_a_request_until_it_closes_then_prints_one_block() {
        let output = render(|printed| {
            let request = info_span!(
                REQUEST_SPAN,
                id = %"4a0f5946",
                method = %"GET",
                path = %"/api/lines/status",
                status = tracing::field::Empty,
                latency_ms = tracing::field::Empty,
            );
            request.in_scope(|| {
                debug!("started processing request");
                info_span!("cache", cache = "mtr.line_status", key = %"all_lines")
                    .in_scope(|| info!(reason = "missing", "refreshing from upstream"));
                request.record("status", 200);
                request.record("latency_ms", 658);
                info!(status = 200, "finished processing request");
            });
            assert_eq!(
                printed(),
                "",
                "nothing is printed while the request is open"
            );
            drop(request);
        });

        let module = module_path!();
        let under_message = " ".repeat("01:17:44.538  INFO ".len());
        assert_eq!(
            output,
            format!(
                "┌─ GET /api/lines/status → 200 in 658 ms\n\
                 │  2026-09-28 01:17:44.538 +08:00  id 4a0f5946\n\
                 │\n\
                 │  01:17:44.538 DEBUG {module}: started processing request\n\
                 │  01:17:44.538  INFO {module}: refreshing from upstream reason=\"missing\"\n\
                 │  {under_message}↳ cache cache=\"mtr.line_status\" key=all_lines\n\
                 │  01:17:44.538  INFO {module}: finished processing request status=200\n\
                 └─\n\n"
            )
        );
    }

    #[test]
    fn keeps_a_request_open_until_background_work_it_started_finishes() {
        let output = render(|printed| {
            let request = info_span!(REQUEST_SPAN, method = %"GET", path = %"/api/lines/status");
            let refresh = info_span!(parent: &request, "cache", key = %"all_lines");
            drop(request);
            assert_eq!(printed(), "", "the refresh still belongs to the request");

            refresh.in_scope(|| info!("cache refreshed"));
            drop(refresh);
        });

        assert!(output.contains("cache refreshed"));
        assert!(output.ends_with("└─\n\n"));
    }

    #[test]
    fn prints_events_outside_a_request_immediately_with_the_full_time() {
        let output = render(|printed| {
            info!(port = 3000, "listening");
            assert_ne!(printed(), "");
        });

        assert_eq!(
            output,
            format!(
                "2026-09-28 01:17:44.538 +08:00  INFO {}: listening port=3000\n\n",
                module_path!()
            )
        );
    }

    #[test]
    fn says_so_when_a_request_closes_without_a_response() {
        let output = render(|_| {
            info_span!(REQUEST_SPAN, method = %"GET", path = %"/api/lines")
                .in_scope(|| debug!("started processing request"));
        });

        assert!(output.starts_with("┌─ GET /api/lines → no response\n"));
    }

    #[test]
    fn shows_the_source_chain_of_an_error_field() {
        #[derive(Debug, thiserror::Error)]
        #[error("upstream request failed")]
        struct Failed(#[source] io::Error);
        let error = Failed(io::Error::new(io::ErrorKind::TimedOut, "timed out"));

        let output = render(|_| warn!(error = &error as &dyn Error, "giving up"));

        assert!(
            output.contains("giving up error=upstream request failed error.sources=[timed out]")
        );
    }
}
