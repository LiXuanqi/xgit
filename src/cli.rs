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
}
