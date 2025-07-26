use crate::{
    git::GitRepo,
    tui::branch_display::{self, BranchInfo, MergeStatus},
};

/// Show statistics for all local branches
pub fn show_branch_stats() -> Result<(), Box<dyn std::error::Error>> {
    let repo = GitRepo::open(".")?;
    let branch_infos = gather_branch_data(&repo)?;
    branch_display::display_branch_stats(&branch_infos);
    Ok(())
}

/// Gather all branch data from the git repository
fn gather_branch_data(repo: &GitRepo) -> Result<Vec<BranchInfo>, Box<dyn std::error::Error>> {
    let branches = repo.get_all_branches()?;
    let current_branch = repo.get_current_branch()?;

    let mut branch_infos = Vec::new();

    for branch in branches {
        let branch_info = BranchInfo {
            name: branch.clone(),
            is_current: branch == current_branch,
            commit_info: repo.get_branch_commit_info(&branch).ok(),
            merge_status: get_merge_status(repo, &branch),
            remote_tracking: repo.get_remote_tracking_info(&branch).ok(),
        };
        branch_infos.push(branch_info);
    }

    Ok(branch_infos)
}

/// Determine the merge status of a branch
fn get_merge_status(repo: &GitRepo, branch: &str) -> MergeStatus {
    match repo.is_branch_merged_to_main(branch) {
        Ok(true) => MergeStatus::Merged,
        Ok(false) => MergeStatus::NotMerged,
        Err(_) => MergeStatus::Unknown,
    }
}
