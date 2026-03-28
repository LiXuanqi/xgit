use crate::{git::GitRepo, github::GitHubPrMatcher};
use console::style;
use inquire::MultiSelect;

#[derive(Debug, Clone)]
struct PruneCandidate {
    branch: String,
    reason: String,
}

/// Prune local branches that have either been merged into trunk or merged via GitHub and deleted remotely.
pub async fn prune_merged_branches(dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
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

    let branches_to_prune = find_branches_to_prune(&repo).await?;

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

async fn find_branches_to_prune(
    repo: &GitRepo,
) -> Result<Vec<PruneCandidate>, Box<dyn std::error::Error>> {
    let all_branches = repo.get_all_branches()?;
    let current_branch = repo.get_current_branch()?;
    let mut branches_to_prune = Vec::new();
    let protected_branches = ["main", "master", "develop"];

    let github_matcher = GitHubPrMatcher::new(repo).ok();
    let mut trunk_branch = None;
    if let Some(ref matcher) = github_matcher {
        let fetch_result = repo.fetch_prune(matcher.remote_name(), None);
        if let Err(err) = fetch_result {
            println!(
                "{} Warning: Failed to refresh remote-tracking branches before squash-merge checks: {}",
                style("⚠").yellow(),
                err
            );
        } else if let Ok(resolved_trunk) = matcher.service().resolve_trunk_base_branch(repo).await {
            trunk_branch = Some(resolved_trunk);
        }
    }

    for branch in all_branches {
        if branch == current_branch {
            continue;
        }
        if protected_branches.contains(&branch.as_str()) {
            continue;
        }

        match repo.is_branch_merged_to_main(&branch) {
            Ok(true) => {
                branches_to_prune.push(PruneCandidate {
                    branch,
                    reason: "merged into local trunk".to_string(),
                });
                continue;
            }
            Ok(false) => {}
            Err(err) => {
                println!(
                    "{} Warning: Could not determine merge status for '{}': {}",
                    style("⚠").yellow(),
                    style(&branch).cyan(),
                    err
                );
            }
        }

        let (Some(matcher), Some(trunk_branch)) = (&github_matcher, trunk_branch.as_deref()) else {
            continue;
        };

        match matcher.refresh_pr_for_branch(repo, &branch).await {
            Ok(Some(resolved_pr))
                if resolved_pr.record.is_merged()
                    && resolved_pr.record.base_ref == trunk_branch
                    && !repo.remote_tracking_branch_exists(&format!(
                        "{}/{}",
                        matcher.remote_name(),
                        resolved_pr.record.head_ref
                    )) =>
            {
                branches_to_prune.push(PruneCandidate {
                    branch,
                    reason: format!(
                        "PR #{} merged to {} and remote head deleted",
                        resolved_pr.record.pr_number, trunk_branch
                    ),
                });
            }
            Ok(_) => {}
            Err(err) => {
                println!(
                    "{} Warning: Could not refresh PR state for '{}': {}",
                    style("⚠").yellow(),
                    style(&branch).cyan(),
                    err
                );
            }
        }
    }

    Ok(branches_to_prune)
}

fn show_dry_run_results(branches_to_prune: &[PruneCandidate]) {
    println!(
        "{} The following {} branches would be deleted:",
        style("📋").cyan().bold(),
        branches_to_prune.len()
    );
    println!();

    for candidate in branches_to_prune {
        println!(
            "  {} {} {}",
            style("🗑").red(),
            style(&candidate.branch).cyan().bold(),
            style(format!("({})", candidate.reason)).dim()
        );
    }

    println!();
    println!(
        "{} Run without --dry-run to actually delete these branches",
        style("💡").blue()
    );
}

fn prune_branches(
    repo: &GitRepo,
    branches_to_prune: &[PruneCandidate],
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{} Found {} merged branches. Select which ones to delete:",
        style("🗑").red().bold(),
        branches_to_prune.len()
    );
    println!();

    for candidate in branches_to_prune {
        println!(
            "  {} {} {}",
            style("•").dim(),
            style(&candidate.branch).cyan().bold(),
            style(format!("({})", candidate.reason)).dim()
        );
    }
    println!();

    let options: Vec<&str> = branches_to_prune
        .iter()
        .map(|candidate| candidate.branch.as_str())
        .collect();
    let branches_to_delete = MultiSelect::new("Select branches to delete:", options).prompt()?;

    if branches_to_delete.is_empty() {
        println!(
            "{} No branches selected for deletion",
            style("ℹ").blue().bold()
        );
        return Ok(());
    }

    println!(
        "{} Deleting {} selected branches:",
        style("🗑").red().bold(),
        branches_to_delete.len()
    );
    println!();

    let mut deleted_count = 0;
    let mut failed_count = 0;

    for branch in branches_to_delete {
        match repo.delete_branch(branch) {
            Ok(()) => {
                println!(
                    "  {} Deleted {}",
                    style("✓").green().bold(),
                    style(branch).cyan()
                );
                deleted_count += 1;
            }
            Err(err) => {
                println!(
                    "  {} Failed to delete {}: {}",
                    style("✗").red().bold(),
                    style(branch).cyan(),
                    err
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
