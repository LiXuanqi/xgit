use crate::git_repo::GitRepo;
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
                        println!("Switched to branch: {}", chosen_branch);
                    }
                    Err(e) => {
                        eprintln!("Error switching to branch '{}': {}", chosen_branch, e);
                    }
                },
                Err(err) => {
                    eprintln!("Selection cancelled: {}", err);
                }
            }
        }
        Err(e) => {
            eprintln!("Error getting branches: {}", e);
        }
    }
    Ok(())
}


