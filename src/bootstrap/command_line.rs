//! The options the process is started with, from its arguments or
//! environment.

use std::{net::SocketAddr, path::PathBuf};

use clap::{Args, Parser, builder::BoolishValueParser};

use dut_telemetry::{LogConfig, LogFilter};

use super::{AppConfig, config::DEFAULT_BIND_ADDRESS};

/// What may differ between two runs of the same build: where the process
/// listens, how it logs, and, as features need them, credentials. What the
/// service serves, such as upstream endpoints and cache policies, is
/// compiled into [`AppConfig`].
///
/// Every option is a flag with an environment variable behind it, and the
/// flag wins, so a container sets options in its environment and a person
/// at a terminal on the command line. Options are grouped by concern, one
/// [`Args`] struct per group under its own help heading; a new concern adds
/// a struct and flattens it here. The doc comments on the groups' fields are
/// the `--help` text.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct CommandLine {
    #[command(flatten)]
    server: ServerOptions,

    #[command(flatten)]
    logging: LoggingOptions,

    #[command(flatten)]
    development: DevelopmentOptions,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Server")]
struct ServerOptions {
    /// IP address and port to listen on. The default accepts connections from
    /// this machine only; `0.0.0.0:3000` or `[::]:3000` accepts them from others
    #[arg(
        long,
        env = "DUT_BIND_ADDRESS",
        value_name = "ADDRESS",
        default_value_t = DEFAULT_BIND_ADDRESS
    )]
    bind_address: SocketAddr,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Logging")]
struct LoggingOptions {
    /// Most detailed level of log events to write: error, warn, info, debug,
    /// or trace. Set a level per crate as in `warn,dut=debug`
    #[arg(
        long,
        env = "RUST_LOG",
        value_name = "LEVEL",
        default_value_t = LogFilter::default()
    )]
    log_level: LogFilter,

    /// Also append log events to this file, one plain line each. The file is
    /// created if missing; its directory must exist
    #[arg(long, env = "DUT_LOG_FILE", value_name = "PATH")]
    log_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Development")]
struct DevelopmentOptions {
    /// Also serve simulated train arrivals and line status under /api/mock,
    /// for developing the app against situations that are rare live, such as
    /// a typhoon signal or a suspended line. The variable accepts true or
    /// false, 1 or 0, yes or no, and on or off
    #[arg(
        long,
        env = "DUT_MOCK_API",
        value_parser = BoolishValueParser::new()
    )]
    mock_api: bool,
}

impl CommandLine {
    /// Reads the process's arguments and environment.
    ///
    /// Asked for `--help` or `--version`, it prints them and exits. Given an
    /// option it cannot use, it prints why and exits with status 2, before
    /// anything has started.
    pub fn read() -> Self {
        Self::parse()
    }

    /// Which log events to write, and where.
    pub fn log_config(&self) -> LogConfig {
        LogConfig {
            filter: self.logging.log_level.clone(),
            file: self.logging.log_file.clone(),
        }
    }

    /// The compiled configuration, adjusted by these options.
    pub fn app_config(&self) -> AppConfig {
        AppConfig::default()
            .with_bind_address(self.server.bind_address)
            .with_mock_api(self.development.mock_api)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsStr,
        iter,
        net::{Ipv4Addr, Ipv6Addr},
    };

    use clap::{CommandFactory, FromArgMatches, error::ErrorKind};

    use super::*;

    /// Parses `args` as if no environment variable were set, so a `RUST_LOG`
    /// in the shell running the tests cannot change their outcome.
    fn parse(args: &[&str]) -> Result<CommandLine, clap::Error> {
        let matches = CommandLine::command()
            .mut_args(|arg| arg.env(None::<&'static str>))
            .try_get_matches_from(iter::once("dut").chain(args.iter().copied()))?;
        CommandLine::from_arg_matches(&matches)
    }

    #[test]
    fn declares_its_options_consistently() {
        CommandLine::command().debug_assert();
    }

    #[test]
    fn changes_nothing_when_no_option_is_given() {
        let command_line = parse(&[]).unwrap();

        assert_eq!(command_line.app_config(), AppConfig::default());
        assert_eq!(command_line.log_config(), LogConfig::default());
    }

    #[test]
    fn applies_every_option_given() {
        let command_line = parse(&[
            "--bind-address",
            "0.0.0.0:8080",
            "--log-level",
            "warn,dut=info",
            "--log-file",
            "/var/log/dut.log",
            "--mock-api",
        ])
        .unwrap();

        assert_eq!(
            command_line.app_config(),
            AppConfig::default()
                .with_bind_address(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8080)))
                .with_mock_api(true)
        );
        assert_eq!(
            command_line.log_config(),
            LogConfig {
                filter: "warn,dut=info".parse().unwrap(),
                file: Some(PathBuf::from("/var/log/dut.log")),
            }
        );
    }

    #[test]
    fn accepts_ipv4_and_ipv6_bind_addresses() {
        let bind_address = |value| {
            parse(&["--bind-address", value])
                .map(|command_line| command_line.server.bind_address)
                .ok()
        };

        assert_eq!(
            bind_address("0.0.0.0:3000"),
            Some(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 3000)))
        );
        assert_eq!(
            bind_address("[::]:8080"),
            Some(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 8080)))
        );
    }

    #[test]
    fn rejects_bind_addresses_without_an_ip_and_port() {
        for value in ["3000", "localhost:3000", "0.0.0.0", ""] {
            let result = parse(&["--bind-address", value]);

            assert_eq!(
                result.map_err(|error| error.kind()).err(),
                Some(ErrorKind::ValueValidation),
                "{value:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_a_mistyped_log_level() {
        let result = parse(&["--log-level", "debgu"]);

        assert_eq!(
            result.map_err(|error| error.kind()).err(),
            Some(ErrorKind::ValueValidation)
        );
    }

    /// Deployments set these names, such as `DUT_BIND_ADDRESS` in the
    /// Dockerfile, so renaming one breaks them.
    #[test]
    fn reads_each_option_from_its_environment_variable() {
        let command = CommandLine::command();
        let variables: Vec<_> = command
            .get_arguments()
            .map(|arg| (arg.get_id().as_str(), arg.get_env()))
            .collect();

        assert_eq!(
            variables,
            [
                ("bind_address", Some(OsStr::new("DUT_BIND_ADDRESS"))),
                ("log_level", Some(OsStr::new("RUST_LOG"))),
                ("log_file", Some(OsStr::new("DUT_LOG_FILE"))),
                ("mock_api", Some(OsStr::new("DUT_MOCK_API"))),
            ]
        );
    }
}
