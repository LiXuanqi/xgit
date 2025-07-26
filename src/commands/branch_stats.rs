use crate::git::GitRepo;
use console::style;
use std::process::Command;

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

        // Get branch status (ahead/behind)
        if let Ok(status) = get_branch_status(&repo, &branch)
            && !status.is_empty()
        {
            println!("  {} {}", style("⚡").yellow(), style(status).yellow());
        }

        println!(); // Empty line between branches
    }

    Ok(())
}

fn get_branch_status(repo: &GitRepo, branch: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Check if we have a remote tracking branch first
    if repo.get_remote_tracking_info(branch).is_err() {
        return Ok(String::new());
    }

    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--branch"])
        .output()?;

    if output.status.success() {
        let status_output = String::from_utf8_lossy(&output.stdout);

        // Parse the first line which contains branch info
        if let Some(first_line) = status_output.lines().next()
            && let Some(branch_info) = first_line.strip_prefix("## ")
        {
            // Look for ahead/behind information
            if branch_info.contains("ahead") || branch_info.contains("behind") {
                // Extract just the ahead/behind part
                if let Some(bracket_start) = branch_info.find('[')
                    && let Some(bracket_end) = branch_info.find(']')
                {
                    return Ok(branch_info[bracket_start + 1..bracket_end].to_string());
                }
            }
        }
    }

    Ok(String::new())
}
