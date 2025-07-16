use super::branch_prune::prune_merged_branches;
use super::branch_stats::show_branch_stats;
use crate::git::GitRepo;
use console::style;
use inquire::Select;

pub fn handle_branch(prune_merged: bool, stats: bool) -> Result<(), Box<dyn std::error::Error>> {
    if prune_merged {
        return prune_merged_branches();
    }

    if stats {
        return show_branch_stats();
    }
    let repo = GitRepo::open(".")?;

    match repo.get_all_branches() {
        Ok(branches) => {
            if branches.is_empty() {
                println!("No branches found");
                return Ok(());
            }

            let selection = Select::new("Select a branch:", branches).prompt();

            match selection {
                Ok(chosen_branch) => match repo.checkout_branch(&chosen_branch) {
                    Ok(()) => {
                        println!(
                            "{} Switched to branch: {}",
                            style("✓").green().bold(),
                            style(&chosen_branch).cyan()
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Error switching to branch '{}': {}",
                            style("✗").red().bold(),
                            style(&chosen_branch).yellow(),
                            style(e).red()
                        );
                    }
                },
                Err(err) => {
                    eprintln!(
                        "{} Selection cancelled: {}",
                        style("⚠").yellow().bold(),
                        style(err).yellow()
                    );
                }
            }
        }
        Err(e) => {
            eprintln!(
                "{} Error getting branches: {}",
                style("✗").red().bold(),
                style(e).red()
            );
        }
    }
    Ok(())
}
