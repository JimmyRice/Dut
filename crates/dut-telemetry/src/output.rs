//! Chooses where log events are written: the console, and a file when asked.

use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, IsTerminal, Stdout},
    path::{Path, PathBuf},
};

use thiserror::Error;
use tracing::{Subscriber, info};
use tracing_subscriber::{
    fmt::{
        self, MakeWriter,
        format::{DefaultFields, Format},
        writer::{OptionalWriter, Tee},
    },
    layer::SubscriberExt,
    registry::LookupSpan,
    util::{SubscriberInitExt, TryInitError},
};

use crate::{filter::LogFilter, request_blocks::RequestBlocks};

/// Which log events are written, and where besides the console.
///
/// The process's caller decides, through the command line or environment;
/// this crate never reads either itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LogConfig {
    pub filter: LogFilter,
    /// Receives every event the console does, one uncoloured line each. It
    /// is appended to, so restarts keep earlier runs' logs, and it is
    /// written as events happen, just as the console is.
    pub file: Option<PathBuf>,
}

/// Why logging could not start.
#[derive(Debug, Error)]
pub enum InitError {
    #[error("cannot open the log file {}: {source}", path.display())]
    OpenFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to install the log subscriber: {0}")]
    Install(#[source] TryInitError),
}

/// Installs a structured logger for the whole process.
///
/// A person watching a terminal gets each request as one block, from
/// "started processing request" to "end of stream", with a summary line and
/// a blank line around it. Anything else, such as a log collector reading
/// standard output, gets one uncoloured line per event, as it happens, so it
/// stays greppable; so does the log file, if there is one.
pub fn init(config: &LogConfig) -> Result<(), InitError> {
    let file = config.file.as_deref().map(open_appending).transpose()?;
    let terminal = io::stdout().is_terminal();
    tracing_subscriber::registry()
        .with(config.filter.to_env_filter())
        .with(terminal.then(|| RequestBlocks::new(io::stdout, colour_wanted())))
        .with(PlainOutput::new(!terminal, file).map(plain_lines))
        .try_init()
        .map_err(InitError::Install)?;

    info!(filter = %config.filter, file = ?config.file, "logging started");
    Ok(())
}

/// Opens `path` for appending, creating it if missing. Every write then
/// lands at the end, even if something else truncates the file meanwhile,
/// as `logrotate`'s `copytruncate` does.
fn open_appending(path: &Path) -> Result<File, InitError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| InitError::OpenFile {
            path: path.to_owned(),
            source,
        })
}

/// One uncoloured line per event, formatted once and written whole to each
/// destination, so lines from concurrent tasks never interleave.
fn plain_lines<S>(output: PlainOutput) -> fmt::Layer<S, DefaultFields, Format, PlainOutput>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fmt::layer().with_ansi(false).with_writer(output)
}

/// Where plain lines go: standard output when nobody watches it in a
/// terminal, the log file, or both.
struct PlainOutput {
    stdout: bool,
    file: Option<File>,
}

impl PlainOutput {
    /// `None` when there is nowhere to write, so that no event is formatted
    /// for nothing.
    fn new(stdout: bool, file: Option<File>) -> Option<Self> {
        (stdout || file.is_some()).then_some(Self { stdout, file })
    }
}

impl<'a> MakeWriter<'a> for PlainOutput {
    type Writer = Tee<OptionalWriter<Stdout>, OptionalWriter<&'a File>>;

    fn make_writer(&'a self) -> Self::Writer {
        Tee::new(
            self.stdout.then(io::stdout).into(),
            self.file.as_ref().into(),
        )
    }
}

/// Honours the `NO_COLOR` convention, as `tracing`'s own formatter does.
fn colour_wanted() -> bool {
    env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// A file in the system's temporary directory, removed when dropped.
    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str) -> Self {
            Self(env::temp_dir().join(format!("dut-telemetry-{}-{name}", std::process::id())))
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn appends_plain_lines_to_a_file_without_erasing_it() {
        let log = TempFile::new("appends.log");
        fs::write(&log.0, "an earlier run\n").unwrap();

        let file = open_appending(&log.0).unwrap();
        let output = PlainOutput::new(false, Some(file)).unwrap();
        let subscriber = tracing_subscriber::registry().with(plain_lines(output));
        tracing::subscriber::with_default(subscriber, || info!(port = 3000, "listening"));

        let written = fs::read_to_string(&log.0).unwrap();
        assert!(written.starts_with("an earlier run\n"), "{written}");
        assert!(
            written.ends_with(&format!(" INFO {}: listening port=3000\n", module_path!())),
            "{written}"
        );
        assert!(!written.contains('\x1b'), "no colour codes: {written:?}");
    }

    #[test]
    fn names_the_file_it_cannot_open() {
        let path = env::temp_dir().join("dut-telemetry-missing-directory/dut.log");

        let result = open_appending(&path);

        assert!(
            matches!(
                result,
                Err(InitError::OpenFile { path: ref rejected, ref source })
                    if *rejected == path && source.kind() == io::ErrorKind::NotFound
            ),
            "{result:?}"
        );
    }
}
