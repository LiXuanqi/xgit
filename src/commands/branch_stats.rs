use crate::git::GitRepo;
use console::style;

/// Show statistics for all local branches
pub fn show_branch_stats() -> Result<(), Box<dyn std::error::Error>> {
    println!("{} Branch Statistics", style("📊").cyan().bold());
    println!();

    // Open git repository and get all branches
    let repo = GitRepo::open(".")?;
    let branches = repo.get_all_branches()?;
    let current_branch = repo.get_current_branch()?;

    if branches.is_empty() {
        println!("{} No branches found", style("⚠").yellow());
        return Ok(());
    }

    for branch in branches {
        // Mark current branch
        let branch_marker = if branch == current_branch {
            style("● ").green().bold()
        } else {
            style("  ").dim()
        };

        println!("{}{}", branch_marker, style(&branch).cyan().bold());

        // Get branch commit info
        if let Ok(commit_info) = repo.get_branch_commit_info(&branch) {
            println!("  {} {}", style("📝").blue(), style(commit_info).dim());
        }

        // Show merge status to main
        match repo.is_branch_merged_to_main(&branch) {
            Ok(true) => println!(
                "  {} {}",
                style("✅").green(),
                style("Merged to main").green()
            ),
            Ok(false) => println!(
                "  {} {}",
                style("🔄").yellow(),
                style("Not merged to main").yellow()
            ),
            Err(_) => {} // Skip if we can't determine merge status
        }

        // TODO: Add GitHub PR lookup back when async is resolved
        println!(
            "  {} {}",
            style("🔗").yellow(),
            style("GitHub PR lookup: TODO").dim()
        );

        // Get remote tracking info
        if let Ok(remote_info) = repo.get_remote_tracking_info(&branch) {
            println!("  {} {}", style("📡").blue(), style(remote_info).cyan());
        } else {
            println!(
                "  {} {}",
                style("📡").blue(),
                style("No remote tracking").yellow()
            );
        }

        println!(); // Empty line between branches
    }

    Ok(())
}
