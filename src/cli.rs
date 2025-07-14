use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gitx")]
#[command(about = "A Git extension tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Branch operations
    Branch {},
    /// Create a commit (passthrough to git commit)
    Commit {
        /// Arguments to pass to git commit
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
