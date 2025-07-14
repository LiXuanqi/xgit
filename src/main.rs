mod ai;
mod cli;
mod commands;
mod git_repo;

#[cfg(test)]
mod test_utils;

use clap::Parser;
use cli::{Cli, Commands};
use console::style;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Branch { prune_merged } => commands::branch::handle_branch(*prune_merged),
        Commands::Commit { args } => commands::commit::handle_commit(args),
        Commands::External(args) => handle_external_command(args),
    };

    if let Err(e) = result {
        eprintln!("{} {}", style("✗").red().bold(), style(e).red());
        std::process::exit(1);
    }
}

fn handle_external_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        eprintln!("{} No command provided", style("✗").red().bold());
        std::process::exit(1);
    }

    let subcommand = &args[0];
    let remaining_args = &args[1..];

    // Allowlist of git commands that are safe to passthrough
    const ALLOWED_COMMANDS: &[&str] = &[
        "add",
        "status",
        "log",
        "diff",
        "show",
        "remote",
        "fetch",
        "pull",
        "push",
        "checkout",
        "switch",
        "merge",
        "rebase",
        "reset",
        "clean",
        "stash",
        "tag",
        "blame",
        "grep",
        "ls-files",
        "describe",
        "reflog",
        "cherry-pick",
        "revert",
        "bisect",
        "submodule",
        "worktree",
        "config",
        "help",
        "version",
    ];

    if ALLOWED_COMMANDS.contains(&subcommand.as_str()) {
        commands::git_passthrough::git_passthrough(subcommand, remaining_args)
    } else {
        eprintln!(
            "{} Command '{}' is not allowed. Use '{}' directly if needed.",
            style("✗").red().bold(),
            style(subcommand).yellow(),
            style(format!("git {subcommand}")).cyan()
        );
        std::process::exit(1);
    }
}
