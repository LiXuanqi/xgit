mod cli;
mod commands;
mod git_repo;

#[cfg(test)]
mod test_utils;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Branch {} => commands::branch::handle_branch(),
        Commands::Commit { args } => commands::commit::handle_commit(args),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
