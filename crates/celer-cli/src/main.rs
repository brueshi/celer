mod commands;
mod pipeline;

use anyhow::Result;
use clap::Parser;

use commands::Command;

#[derive(Debug, Parser)]
#[command(
    name = "celerate",
    about = "Celer -- AOT Python compiler targeting backend frameworks",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Compile(args) => commands::compile::execute(&args),
        Command::Run(args) => commands::run::execute(&args),
        Command::Bench(args) => commands::bench::execute(&args),
    }
}
