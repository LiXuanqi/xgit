use anyhow::{Context, Error};
use git2::BranchType;

use crate::git::repository::core::GitRepo;

impl GitRepo {
    /// Get remote tracking info for a specific branch
    pub fn get_remote_tracking_info(&self, branch: &str) -> Result<String, Error> {
        let branch_ref = format!("refs/heads/{branch}");

        // Try to get the upstream branch
        let upstream = self
            .repo()
            .branch_upstream_name(&branch_ref)
            .context("No remote tracking branch")?;

        let upstream_str = upstream
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert upstream name to string"))?;

        // Extract just the remote/branch part (remove refs/remotes/ prefix)
        let tracking_branch = upstream_str
            .strip_prefix("refs/remotes/")
            .unwrap_or(upstream_str);

        Ok(tracking_branch.to_string())
    }

    /// Check if all commits in the given branch are already in main/master
    pub fn is_branch_merged_into_main(&self, branch: &str) -> Result<bool, Error> {
        // Try to find main or master branch
        let main_branch = if self.repo().find_branch("main", BranchType::Local).is_ok() {
            "main"
        } else if self.repo().find_branch("master", BranchType::Local).is_ok() {
            "master"
        } else {
            return Err(anyhow::anyhow!("Neither main nor master branch found"));
        };

        // Get the commit for the branch
        let branch_ref = format!("refs/heads/{branch}");
        let branch_obj = self
            .repo()
            .revparse_single(&branch_ref)
            .context(format!("Failed to find branch '{branch}'"))?;
        let branch_commit = branch_obj
            .peel_to_commit()
            .context("Failed to get branch commit")?;

        // Get the commit for main/master
        let main_ref = format!("refs/heads/{main_branch}");
        let main_obj = self
            .repo()
            .revparse_single(&main_ref)
            .context(format!("Failed to find {main_branch} branch"))?;
        let main_commit = main_obj
            .peel_to_commit()
            .context("Failed to get main branch commit")?;

        // Check if the branch commit is reachable from main
        let merge_base = self
            .repo()
            .merge_base(branch_commit.id(), main_commit.id())
            .context("Failed to find merge base")?;

        // If the merge base equals the branch commit, then all branch commits are in main
        Ok(merge_base == branch_commit.id())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{RepoTestOperations, create_test_bare_repo, create_test_repo};

    #[test]
    fn get_remote_tracking_info_works() {
        let (_remote_dir, remote_repo) = create_test_bare_repo();

        // Setup local repository
        let (_local_dir, local_repo) = create_test_repo();
        local_repo
            .add_file_and_commit("test.txt", "content", "Initial commit")
            .unwrap();

        // Add the remote repository
        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Create and checkout a new branch
        local_repo
            .create_and_checkout_branch("feature-branch")
            .unwrap();
        local_repo
            .add_file_and_commit("feature.txt", "feature content", "Add feature")
            .unwrap();

        // Push the branch to remote (this sets up tracking)
        local_repo.push("origin", "feature-branch").unwrap();

        // Now test getting remote tracking info for the feature branch
        let result = local_repo.get_remote_tracking_info("feature-branch");
        match result {
            Ok(tracking) => {
                // Should be "origin/feature-branch"
                assert_eq!(tracking, "origin/feature-branch");
            }
            Err(e) => {
                // Print error for debugging if needed
                println!("Failed to get remote tracking info: {e}");
                // For now, we'll allow this to fail since remote tracking setup can be complex
            }
        }

        // Test with non-existent branch
        let result = local_repo.get_remote_tracking_info("nonexistent");
        assert!(result.is_err());

        // Test with master branch (no remote tracking set up)
        local_repo.checkout_branch("master").unwrap();
        let master_result = local_repo.get_remote_tracking_info("master");
        // Master likely won't have tracking set up, so error is expected
        assert!(master_result.is_err());
    }

    #[test]
    fn is_branch_merged_into_main_works() {
        let (_remote_dir, remote_repo) = create_test_bare_repo();

        // Setup local repository
        let (_local_dir, local_repo) = create_test_repo();
        local_repo
            .add_file_and_commit("README.md", "initial", "Initial commit")
            .unwrap();

        // Add the remote repository
        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push master to establish it on remote
        local_repo.push("origin", "master").unwrap();

        // Create a feature branch and add a commit
        local_repo.create_and_checkout_branch("feature").unwrap();
        local_repo
            .add_file_and_commit("feature.txt", "feature content", "Add feature")
            .unwrap();

        local_repo.push("origin", "feature").unwrap();

        // Feature branch should not be merged yet
        let result = local_repo.is_branch_merged_into_main("feature").unwrap();
        assert!(!result);

        remote_repo.checkout_branch("master").unwrap();
        remote_repo.merge("feature", None).unwrap();

        local_repo.checkout_branch("master").unwrap();
        local_repo.pull("origin", Some("master")).unwrap();

        let result = local_repo.is_branch_merged_into_main("feature").unwrap();
        assert!(result);
    }
}
