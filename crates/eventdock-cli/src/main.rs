mod cli;
mod commands;
mod config;

use clap::Parser;
use cli::{Cli, Command};
use commands::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    match Cli::parse().command {
        Command::Serve(args) => commands::serve::run(args).await,
        Command::Send(args) => commands::send::run(args),
        Command::Query(args) => commands::query::run(args),
    }
}
