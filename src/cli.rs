use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xgit")]
#[command(about = "A Git extension tool")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Branch operations (alias: b)
    #[command(alias = "b")]
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
    /// Create a commit (passthrough to git commit) (alias: c)
    #[command(alias = "c")]
    Commit {
        /// Arguments to pass to git commit
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Sync local commit stack to GitHub stacked PRs
    Diff {
        /// Repair mapping by attaching a PR number to a commit SHA and resyncing
        #[arg(long, value_names = ["PR_NUMBER", "COMMIT_SHA"], num_args = 2)]
        repair: Option<Vec<String>>,
    },
    /// Explicit git passthrough command (e.g. xgit git diff)
    Git {
        /// Git arguments where first arg is the git subcommand
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
