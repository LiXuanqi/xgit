use anyhow::{Context, Error};

use crate::git::repository::core::GitRepo;

impl GitRepo {
    /// Merge a branch into the current branch
    pub fn merge(&self, branch_name: &str, message: Option<&str>) -> Result<String, Error> {
        let signature = self
            .create_signature()
            .context("Failed to create signature")?;

        // Get the target branch to merge
        let branch_ref = format!("refs/heads/{branch_name}");
        let target_obj = self
            .repo()
            .revparse_single(&branch_ref)
            .context(format!("Failed to find branch '{branch_name}'"))?;
        let target_commit = target_obj
            .peel_to_commit()
            .context("Failed to get target commit")?;

        // Get the current branch commit
        let head_ref = self.repo().head().context("Failed to get HEAD")?;
        let head_commit = head_ref
            .peel_to_commit()
            .context("Failed to get current commit")?;

        // Check if already up-to-date
        if head_commit.id() == target_commit.id() {
            return Ok("Already up-to-date".to_string());
        }

        // Check if fast-forward is possible
        let merge_base = self
            .repo()
            .merge_base(head_commit.id(), target_commit.id())
            .context("Failed to find merge base")?;

        if merge_base == head_commit.id() {
            // Fast-forward merge: update branch reference to target commit
            let current_branch_name = self
                .get_current_branch()
                .context("Failed to get current branch")?;

            // Create a new reference for the branch pointing to target commit
            let branch_ref_name = format!("refs/heads/{current_branch_name}");
            self.repo()
                .reference(
                    &branch_ref_name,
                    target_commit.id(),
                    true,
                    "Fast-forward merge",
                )
                .context("Failed to update branch reference")?;

            // Update working directory if not bare
            if !self.is_bare() {
                let target_tree = target_commit.tree().context("Failed to get target tree")?;
                let mut checkout_opts = git2::build::CheckoutBuilder::new();
                checkout_opts.force();
                self.repo()
                    .checkout_tree(target_tree.as_object(), Some(&mut checkout_opts))
                    .context("Failed to checkout target tree")?;
            }

            Ok(format!(
                "Fast-forward merge: {target_commit_id}",
                target_commit_id = target_commit.id()
            ))
        } else if merge_base == target_commit.id() {
            // Already up to date
            Ok("Already up-to-date".to_string())
        } else {
            // True merge required
            let head_tree = head_commit.tree().context("Failed to get HEAD tree")?;

            // Perform three-way merge
            let mut index = self.repo().index().context("Failed to get index")?;
            index
                .read_tree(&head_tree)
                .context("Failed to read head tree")?;

            // Use git2's merge functionality through repository
            let mut merge_options = git2::MergeOptions::new();
            let mut checkout_opts = git2::build::CheckoutBuilder::new();
            checkout_opts.conflict_style_merge(true);

            let annotated_commit = self
                .repo()
                .find_annotated_commit(target_commit.id())
                .context("Failed to create annotated commit")?;

            // Perform the merge analysis
            let (analysis, _) = self
                .repo()
                .merge_analysis(&[&annotated_commit])
                .context("Failed to analyze merge")?;

            if analysis.is_up_to_date() {
                Ok("Already up-to-date".to_string())
            } else if analysis.is_fast_forward() {
                // This shouldn't happen since we checked above, but handle it
                self.repo()
                    .set_head_detached(target_commit.id())
                    .context("Failed to fast-forward merge")?;
                Ok(format!(
                    "Fast-forward merge: {target_commit_id}",
                    target_commit_id = target_commit.id()
                ))
            } else if analysis.is_normal() {
                // Perform actual merge
                self.repo()
                    .merge(
                        &[&annotated_commit],
                        Some(&mut merge_options),
                        Some(&mut checkout_opts),
                    )
                    .context("Failed to perform merge")?;

                // Check for conflicts
                let mut index = self
                    .repo()
                    .index()
                    .context("Failed to get index after merge")?;
                if index.has_conflicts() {
                    return Err(anyhow::anyhow!(
                        "Merge conflicts detected. Please resolve conflicts and commit manually."
                    ));
                }

                // Create merge commit
                let tree_id = index.write_tree().context("Failed to write merge tree")?;
                let tree = self
                    .repo()
                    .find_tree(tree_id)
                    .context("Failed to find merge tree")?;

                let default_message = format!("Merge branch '{branch_name}'");
                let commit_message = message.unwrap_or(&default_message);

                let merge_commit_id = self
                    .repo()
                    .commit(
                        Some("HEAD"),
                        &signature,
                        &signature,
                        commit_message,
                        &tree,
                        &[&head_commit, &target_commit],
                    )
                    .context("Failed to create merge commit")?;

                // Clean up merge state
                self.repo()
                    .cleanup_state()
                    .context("Failed to cleanup merge state")?;

                Ok(format!("Merge commit created: {merge_commit_id}"))
            } else {
                Err(anyhow::anyhow!("Unsupported merge analysis result"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{create_test_repo, RepoAssertions, RepoTestOperations};

    #[test]
    fn merge_works() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        // Add initial commit to master
        repo.add_file_and_commit("README.md", "initial", "Initial commit")?
            .create_and_checkout_branch("feature")?
            .add_file_and_commit("feature.txt", "feature content", "Add feature")?
            .checkout_branch("master")?;

        // Merge the feature branch
        let result = repo.merge("feature", None).unwrap();
        assert!(result.contains("Fast-forward merge") || result.contains("Merge commit created"));

        // Verify the feature file exists on master after merge
        repo.assert_file_exists("feature.txt");

        // Test merging already merged branch
        let result = repo.merge("feature", None).unwrap();
        assert_eq!(result, "Already up-to-date");

        // Test merging non-existent branch
        let result = repo.merge("nonexistent", None);
        assert!(result.is_err());
        Ok(())
    }
}
