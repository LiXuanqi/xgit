use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gitx")]
#[command(about = "A Git extension tool")]
#[command(allow_external_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Branch operations
    Branch {
        /// Clean up local branches that have been merged and deleted remotely
        #[arg(long)]
        prune_merged: bool,
        /// Show current branch and associated GitHub PR information
        #[arg(long)]
        stats: bool,
        /// Show what would be pruned without actually deleting branches
        #[arg(long)]
        dry_run: bool,
    },
    /// Create a commit (passthrough to git commit)
    Commit {
        /// Arguments to pass to git commit
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// External subcommands (passthrough to git)
    #[command(external_subcommand)]
    External(Vec<String>),
}
