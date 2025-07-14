use console::style;
use std::process::Command;

/// Helper function to passthrough commands to git
pub fn git_passthrough(
    subcommand: &str,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new("git");
    cmd.arg(subcommand);
    cmd.args(args);

    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Err(e) => {
            eprintln!(
                "{} Error running git {}: {}",
                style("✗").red().bold(),
                style(subcommand).cyan(),
                style(e).red()
            );
            std::process::exit(1);
        }
    }

    Ok(())
}
