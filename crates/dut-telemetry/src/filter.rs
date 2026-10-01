//! Which log events are written.

use std::{fmt, str::FromStr};

use thiserror::Error;
use tracing_subscriber::{
    EnvFilter,
    filter::{LevelFilter, ParseError},
};

/// This service and HTTP request logs at debug level, which includes cache
/// hits, and everything else at info. Targets match by prefix, so `dut` also
/// covers every `dut_*` crate.
const DEFAULT_FILTER: &str = "info,dut=debug,tower_http=debug";

/// A level such as `info`, optionally refined per target, as in
/// `warn,dut=debug`: the syntax of `RUST_LOG`.
///
/// It is checked when parsed, so a mistyped level stops the process at
/// startup instead of silently logging something other than what was asked.
/// That is stricter than `RUST_LOG` itself in two ways, both of which would
/// otherwise turn logging off without a word: a filter must not be empty,
/// and a bare word must be a level, since `RUST_LOG` reads `debgu` as a
/// target to log alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogFilter(String);

/// Why a log filter was rejected.
#[derive(Debug, Error)]
pub enum InvalidLogFilter {
    #[error("the filter is empty, which would write no logs; say `off` to mean that")]
    Empty,

    #[error(
        "{0:?} is not a level, one of off, error, warn, info, debug, or trace; \
         set one target's level with target=level"
    )]
    NotALevel(String),

    #[error(transparent)]
    Syntax(#[from] ParseError),
}

impl LogFilter {
    pub(crate) fn to_env_filter(&self) -> EnvFilter {
        // Parsing already succeeded once, in `from_str`, so nothing is lost.
        EnvFilter::builder().parse_lossy(&self.0)
    }
}

impl Default for LogFilter {
    fn default() -> Self {
        Self(DEFAULT_FILTER.to_owned())
    }
}

impl FromStr for LogFilter {
    type Err = InvalidLogFilter;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // `EnvFilter` skips empty directives too, so `info,` is `info`.
        let mut directives = value
            .split(',')
            .filter(|directive| !directive.is_empty())
            .peekable();
        if directives.peek().is_none() {
            return Err(InvalidLogFilter::Empty);
        }
        if let Some(target) = directives.find(|directive| names_a_target_alone(directive)) {
            return Err(InvalidLogFilter::NotALevel(target.to_owned()));
        }
        EnvFilter::builder().parse(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for LogFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `directive` enables one target at every level and leaves the rest
/// off, such as `dut`. Directives that set a level contain `=`, and span
/// filters contain `[`.
fn names_a_target_alone(directive: &str) -> bool {
    !directive.contains(['=', '[']) && directive.parse::<LevelFilter>().is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_level_alone_or_refined_per_target() {
        for value in [
            "warn",
            "TRACE",
            "off",
            "info,",
            "info,dut=debug",
            "dut_upstream=trace",
            "info,[request]",
        ] {
            assert_eq!(
                value
                    .parse::<LogFilter>()
                    .map(|filter| filter.to_string())
                    .ok(),
                Some(value.to_owned()),
                "{value:?} should be accepted"
            );
        }
    }

    #[test]
    fn rejects_a_bare_word_that_is_not_a_level() {
        for (value, word) in [("debgu", "debgu"), ("warn,dut", "dut")] {
            assert!(
                matches!(
                    value.parse::<LogFilter>(),
                    Err(InvalidLogFilter::NotALevel(ref rejected)) if rejected == word
                ),
                "{value:?} should be rejected for {word:?}"
            );
        }
    }

    #[test]
    fn rejects_a_filter_that_would_silence_everything() {
        for value in ["", ","] {
            assert!(
                matches!(value.parse::<LogFilter>(), Err(InvalidLogFilter::Empty)),
                "{value:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_levels_that_do_not_exist_and_malformed_directives() {
        for value in ["dut=loud", "info, dut=debug"] {
            assert!(
                matches!(value.parse::<LogFilter>(), Err(InvalidLogFilter::Syntax(_))),
                "{value:?} should be rejected"
            );
        }
    }

    #[test]
    fn the_default_round_trips_through_its_text() {
        let default = LogFilter::default();

        assert_eq!(default.to_string().parse::<LogFilter>().ok(), Some(default));
    }
}
