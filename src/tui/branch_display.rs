use console::style;
use serde::{Deserialize, Serialize};

/// Information about a single branch
#[derive(Debug)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub commit_info: Option<String>,
    pub merge_status: MergeStatus,
    pub remote_tracking: Option<String>,
    pub pull_request: Option<PullRequestInfo>,
}

/// Merge status of a branch relative to main
#[derive(Debug)]
pub enum MergeStatus {
    Merged,
    NotMerged,
    Unknown,
}

/// Information about a GitHub pull request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub state: PullRequestState,
    pub url: String,
    pub draft: bool,
}

/// State of a GitHub pull request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PullRequestState {
    Open,
    Closed,
}

/// Display branch statistics in a formatted way
pub fn display_branch_stats(branches: &[BranchInfo]) {
    println!("{} Branch Statistics", style("📊").cyan().bold());
    println!();

    if branches.is_empty() {
        println!("{} No branches found", style("⚠").yellow());
        return;
    }

    for branch in branches {
        display_single_branch(branch);
    }
}

/// Display information for a single branch
fn display_single_branch(branch: &BranchInfo) {
    // Mark current branch
    let branch_marker = if branch.is_current {
        style("● ").green().bold()
    } else {
        style("  ").dim()
    };

    println!("{}{}", branch_marker, style(&branch.name).cyan().bold());

    // Display commit info
    if let Some(commit_info) = &branch.commit_info {
        println!("  {} {}", style("📝").blue(), style(commit_info).dim());
    }

    // Show merge status to main
    display_merge_status(&branch.merge_status);

    // Display GitHub PR information
    display_pull_request_info(&branch.pull_request);

    // Display remote tracking info
    display_remote_tracking_info(&branch.remote_tracking);

    println!(); // Empty line between branches
}

/// Display merge status for a branch
fn display_merge_status(status: &MergeStatus) {
    match status {
        MergeStatus::Merged => println!(
            "  {} {}",
            style("✅").green(),
            style("Merged to main").green()
        ),
        MergeStatus::NotMerged => println!(
            "  {} {}",
            style("🔄").yellow(),
            style("Not merged to main").yellow()
        ),
        MergeStatus::Unknown => {} // Skip if we can't determine merge status
    }
}

/// Display GitHub pull request information for a branch
fn display_pull_request_info(pull_request: &Option<PullRequestInfo>) {
    if let Some(pr) = pull_request {
        let state_display = match pr.state {
            PullRequestState::Open => {
                if pr.draft {
                    style("Draft").yellow()
                } else {
                    style("Open").green()
                }
            }
            PullRequestState::Closed => style("Closed").red(),
        };

        println!(
            "  {} {} {} {}",
            style("🔗").yellow(),
            style(format!("PR #{}", pr.number)).cyan().bold(),
            state_display,
            style(&pr.title).dim()
        );
    } else {
        println!(
            "  {} {}",
            style("🔗").yellow(),
            style("No GitHub PR found").dim()
        );
    }
}

/// Display remote tracking information for a branch
fn display_remote_tracking_info(remote_tracking: &Option<String>) {
    if let Some(remote_info) = remote_tracking {
        println!("  {} {}", style("📡").blue(), style(remote_info).cyan());
    } else {
        println!(
            "  {} {}",
            style("📡").blue(),
            style("No remote tracking").yellow()
        );
    }
}
