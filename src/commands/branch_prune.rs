use crate::git::GitRepo;
use console::style;

/// Prune local branches that have been merged to main
///
/// This function:
/// - Finds all local branches that are merged to main
/// - Skips main/master and current branch for safety
/// - Shows what will be deleted (dry-run mode) or actually deletes branches
/// - Provides clear user feedback about what's being deleted and why
pub fn prune_merged_branches(dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    let repo = GitRepo::open(".")?;

    println!(
        "{} {}",
        style("🔍").blue().bold(),
        if dry_run {
            "Finding branches that would be pruned (dry run)..."
        } else {
            "Finding merged branches to prune..."
        }
    );
    println!();

    let branches_to_prune = find_branches_to_prune(&repo)?;

    if branches_to_prune.is_empty() {
        println!(
            "{} No merged branches found to prune",
            style("✨").green().bold()
        );
        return Ok(());
    }

    if dry_run {
        show_dry_run_results(&branches_to_prune);
    } else {
        prune_branches(&repo, &branches_to_prune)?;
    }

    Ok(())
}

/// Find branches that can be safely pruned
fn find_branches_to_prune(repo: &GitRepo) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let all_branches = repo.get_all_branches()?;
    let current_branch = repo.get_current_branch()?;
    let mut branches_to_prune = Vec::new();

    // Protected branches that should never be pruned
    let protected_branches = ["main", "master", "develop"];

    for branch in all_branches {
        // Skip current branch
        if branch == current_branch {
            continue;
        }

        // Skip protected branches
        if protected_branches.contains(&branch.as_str()) {
            continue;
        }

        // Check if branch is merged to main
        match repo.is_branch_merged_to_main(&branch) {
            Ok(true) => {
                branches_to_prune.push(branch);
            }
            Ok(false) => {
                // Branch not merged, skip
            }
            Err(e) => {
                println!(
                    "{} Warning: Could not determine merge status for '{}': {}",
                    style("⚠").yellow(),
                    style(&branch).cyan(),
                    e
                );
            }
        }
    }

    Ok(branches_to_prune)
}

/// Show what would be pruned in dry-run mode
fn show_dry_run_results(branches_to_prune: &[String]) {
    println!(
        "{} The following {} branches would be deleted:",
        style("📋").cyan().bold(),
        branches_to_prune.len()
    );
    println!();

    for branch in branches_to_prune {
        println!(
            "  {} {} {}",
            style("🗑").red(),
            style(branch).cyan().bold(),
            style("(merged to main)").dim()
        );
    }

    println!();
    println!(
        "{} Run without --dry-run to actually delete these branches",
        style("💡").blue()
    );
}

/// Actually prune the branches
fn prune_branches(
    repo: &GitRepo,
    branches_to_prune: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{} Deleting {} merged branches:",
        style("🗑").red().bold(),
        branches_to_prune.len()
    );
    println!();

    let mut deleted_count = 0;
    let mut failed_count = 0;

    for branch in branches_to_prune {
        match repo.delete_branch(branch) {
            Ok(()) => {
                println!(
                    "  {} Deleted {}",
                    style("✓").green().bold(),
                    style(branch).cyan()
                );
                deleted_count += 1;
            }
            Err(e) => {
                println!(
                    "  {} Failed to delete {}: {}",
                    style("✗").red().bold(),
                    style(branch).cyan(),
                    e
                );
                failed_count += 1;
            }
        }
    }

    println!();
    println!(
        "{} Deleted {} branches{}",
        style("✨").green().bold(),
        deleted_count,
        if failed_count > 0 {
            format!(", {failed_count} failed")
        } else {
            String::new()
        }
    );

    Ok(())
}
