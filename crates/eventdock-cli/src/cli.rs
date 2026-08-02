use crate::config::{
    DEFAULT_ADDRESS, DEFAULT_EVENT_CAPACITY, DEFAULT_QUERY_ADDRESS,
    DEFAULT_QUERY_LIMIT,
};

use clap::{Parser, Subcommand, ValueEnum};
use eventdock::LogLevel;

#[derive(Debug, Parser)]
#[command(name = "eventdock")]
#[command(about = "Receive, inspect, and send structured application events")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve(ServeArgs),
    Send(SendArgs),
    Query(QueryArgs),
}

#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    #[arg(long, default_value = DEFAULT_ADDRESS)]
    pub address: String,

    #[arg(long, default_value = DEFAULT_QUERY_ADDRESS)]
    pub query_address: String,

    #[arg(long, default_value_t = DEFAULT_EVENT_CAPACITY)]
    pub capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct SendArgs {
    #[arg(long, default_value = DEFAULT_ADDRESS)]
    pub address: String,

    #[arg(long, default_value = "app")]
    pub source: String,

    #[arg(long, value_enum, default_value_t = CliLogLevel::Info)]
    pub level: CliLogLevel,

    #[arg(long)]
    pub message: String,

    #[arg(long = "field", value_name = "KEY=VALUE")]
    pub fields: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub struct QueryArgs {
    #[arg(long, default_value = DEFAULT_QUERY_ADDRESS)]
    pub address: String,

    #[arg(long, default_value_t = DEFAULT_QUERY_LIMIT)]
    pub limit: usize,

    #[arg(long)]
    pub before_id: Option<u64>,

    #[arg(long, value_enum)]
    pub level: Option<CliLogLevel>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliLogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl From<CliLogLevel> for LogLevel {
    fn from(level: CliLogLevel) -> Self {
        match level {
            CliLogLevel::Trace => Self::Trace,
            CliLogLevel::Debug => Self::Debug,
            CliLogLevel::Info => Self::Info,
            CliLogLevel::Warning => Self::Warning,
            CliLogLevel::Error => Self::Error,
            CliLogLevel::Critical => Self::Critical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_defaults() {
        let cli = Cli::try_parse_from(["eventdock", "serve"]).unwrap();

        let Command::Serve(args) = cli.command else {
            panic!("expected serve")
        };

        assert_eq!(args.address, DEFAULT_ADDRESS);
        assert_eq!(args.query_address, DEFAULT_QUERY_ADDRESS);
        assert_eq!(args.capacity, DEFAULT_EVENT_CAPACITY);
    }

    #[test]
    fn send_parses_repeated_fields() {
        let cli = Cli::try_parse_from([
            "eventdock",
            "send",
            "--message",
            "hello",
            "--field",
            "a=1",
            "--field",
            "b=two",
        ])
        .unwrap();

        let Command::Send(args) = cli.command else {
            panic!("expected send")
        };

        assert_eq!(args.fields, ["a=1", "b=two"]);
        assert_eq!(args.message, "hello");
    }

    #[test]
    fn query_parses_level_and_cursor() {
        let cli = Cli::try_parse_from([
            "eventdock",
            "query",
            "--before-id",
            "42",
            "--level",
            "warning",
        ])
        .unwrap();

        let Command::Query(args) = cli.command else {
            panic!("expected query")
        };

        assert_eq!(args.before_id, Some(42));
        assert!(matches!(args.level, Some(CliLogLevel::Warning)));
    }
}
