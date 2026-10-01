//! Tells the operator how the process ended.

use std::{
    error::Error,
    fmt,
    io::{self, Write},
    iter,
    process::ExitCode,
};

use super::StartupError;

/// Turns how [`run`](super::run) ended into the process's exit status. A
/// failure is also written to standard error as one line, such as `error:
/// failed to start logging: cannot open the log file /var/log/dut.log: No
/// such file or directory (os error 2)`, prefixed `error:` as the command
/// line's own usage errors are.
///
/// It writes to standard error directly rather than logging, since logging
/// may be what failed to start. If even that write fails, nothing is left
/// to tell, and the status alone reports the failure.
pub fn report(outcome: Result<(), StartupError>) -> ExitCode {
    report_to(&mut io::stderr(), outcome)
}

/// [`report`], writing to `out`, so tests can read what it writes.
fn report_to(out: &mut impl Write, outcome: Result<(), StartupError>) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(out, "error: {}", WithCauses(&err));
            ExitCode::FAILURE
        }
    }
}

/// Displays an error followed by each of its causes, outermost first,
/// separated by colons. Each message must leave out its own cause, as every
/// error in the workspace does, or that cause would appear twice.
struct WithCauses<'a>(&'a dyn Error);

impl fmt::Display for WithCauses<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        for cause in iter::successors(self.0.source(), |&cause| cause.source()) {
            write!(f, ": {cause}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{io, net::SocketAddr, path::PathBuf};

    use dut_telemetry::InitError;

    use super::*;

    /// What [`report_to`] writes for `outcome`, and the status it returns.
    fn reported(outcome: Result<(), StartupError>) -> (String, ExitCode) {
        let mut out = Vec::new();
        let status = report_to(&mut out, outcome);
        (String::from_utf8(out).unwrap(), status)
    }

    #[test]
    fn writes_a_failure_on_one_line_with_each_cause_once() {
        let err = StartupError::Telemetry(InitError::OpenFile {
            path: PathBuf::from("/var/log/dut/dut.log"),
            source: io::ErrorKind::NotFound.into(),
        });

        assert_eq!(
            reported(Err(err)),
            (
                "error: failed to start logging: \
                 cannot open the log file /var/log/dut/dut.log: entity not found\n"
                    .to_owned(),
                ExitCode::FAILURE
            )
        );
    }

    #[test]
    fn names_the_address_it_cannot_bind() {
        let err = StartupError::Bind {
            address: SocketAddr::from(([127, 0, 0, 1], 3000)),
            source: io::ErrorKind::AddrInUse.into(),
        };

        assert_eq!(
            WithCauses(&err).to_string(),
            "failed to bind the HTTP server to 127.0.0.1:3000: address in use"
        );
    }

    #[test]
    fn displays_an_error_without_a_cause_alone() {
        let err = io::Error::from(io::ErrorKind::AddrInUse);

        assert_eq!(WithCauses(&err).to_string(), "address in use");
    }

    #[test]
    fn exits_successfully_and_quietly_after_a_clean_shutdown() {
        assert_eq!(reported(Ok(())), (String::new(), ExitCode::SUCCESS));
    }
}
