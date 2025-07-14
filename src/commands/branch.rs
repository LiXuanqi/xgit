use crate::git_repo::GitRepo;
use console::style;
use inquire::Select;

pub fn handle_branch() -> Result<(), Box<dyn std::error::Error>> {
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
