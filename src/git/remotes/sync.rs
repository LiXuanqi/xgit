use anyhow::{Context, Error};

use crate::git::repository::core::GitRepo;

impl GitRepo {
    /// Fetch changes from a remote repository
    pub fn fetch(&self, remote_name: &str, branch_name: Option<&str>) -> Result<String, Error> {
        let mut remote = self
            .repo()
            .find_remote(remote_name)
            .context(format!("Remote '{remote_name}' not found"))?;

        let refspecs = match branch_name {
            Some(branch) => {
                // Fetch specific branch
                vec![format!(
                    "refs/heads/{branch}:refs/remotes/{remote_name}/{branch}"
                )]
            }
            None => {
                // Fetch all branches according to remote's default refspecs
                let refspecs = remote
                    .fetch_refspecs()
                    .context("Failed to get remote refspecs")?;

                let mut result = Vec::new();
                for i in 0..refspecs.len() {
                    if let Some(refspec) = refspecs.get(i) {
                        result.push(refspec.to_string());
                    }
                }
                result
            }
        };

        let refspecs: Vec<&str> = refspecs.iter().map(|s| s.as_str()).collect();

        // Perform the fetch
        remote
            .fetch(&refspecs, None, None)
            .context("Failed to fetch from remote")?;

        // Get fetch statistics
        let stats = remote.stats();
        let received_objects = stats.received_objects();
        let total_objects = stats.total_objects();

        if received_objects > 0 {
            Ok(format!(
                "Fetched {received_objects}/{total_objects} objects from {remote_name}"
            ))
        } else {
            Ok("Already up-to-date".to_string())
        }
    }

    /// Pull changes from a remote repository (fetch + merge)
    pub fn pull(&self, remote_name: &str, branch_name: Option<&str>) -> Result<String, Error> {
        // Get current branch if no branch specified
        let target_branch = match branch_name {
            Some(branch) => branch.to_string(),
            None => self
                .get_current_branch()
                .context("Failed to get current branch")?,
        };

        // Fetch from remote first
        self.fetch(remote_name, Some(&target_branch))
            .context("Failed to fetch from remote")?;

        // Get the remote tracking branch
        let remote_branch = format!("{remote_name}/{target_branch}");

        // Check if remote branch exists
        let remote_ref = format!("refs/remotes/{remote_branch}");
        let remote_obj = self.repo().revparse_single(&remote_ref).context(format!(
            "Remote branch '{remote_branch}' not found after fetch"
        ))?;
        let remote_commit = remote_obj
            .peel_to_commit()
            .context("Failed to get remote commit")?;

        // Get current branch commit
        let head_ref = self.repo().head().context("Failed to get HEAD")?;
        let head_commit = head_ref
            .peel_to_commit()
            .context("Failed to get current commit")?;

        // Check if already up-to-date
        if head_commit.id() == remote_commit.id() {
            return Ok("Already up-to-date".to_string());
        }

        // Check if fast-forward is possible
        let merge_base = self
            .repo()
            .merge_base(head_commit.id(), remote_commit.id())
            .context("Failed to find merge base")?;

        if merge_base == head_commit.id() {
            // Fast-forward pull: update branch reference to remote commit
            let current_branch_name = self
                .get_current_branch()
                .context("Failed to get current branch")?;

            // Create a new reference for the branch pointing to remote commit
            let branch_ref_name = format!("refs/heads/{current_branch_name}");
            self.repo()
                .reference(
                    &branch_ref_name,
                    remote_commit.id(),
                    true,
                    "Fast-forward pull",
                )
                .context("Failed to update branch reference")?;

            // Update working directory if not bare
            if !self.is_bare() {
                let remote_tree = remote_commit.tree().context("Failed to get remote tree")?;
                let mut checkout_opts = git2::build::CheckoutBuilder::new();
                checkout_opts.force();
                self.repo()
                    .checkout_tree(remote_tree.as_object(), Some(&mut checkout_opts))
                    .context("Failed to checkout remote tree")?;
            }

            Ok(format!(
                "Fast-forward pull: {remote_commit_id}",
                remote_commit_id = remote_commit.id()
            ))
        } else if merge_base == remote_commit.id() {
            // Local branch is ahead of remote
            Ok("Already up-to-date".to_string())
        } else {
            // Need to merge remote changes
            let signature = self
                .create_signature()
                .context("Failed to create signature")?;

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
                .find_annotated_commit(remote_commit.id())
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
                    .reference(
                        &format!("refs/heads/{target_branch}"),
                        remote_commit.id(),
                        true,
                        "Fast-forward pull",
                    )
                    .context("Failed to fast-forward pull")?;
                Ok(format!(
                    "Fast-forward pull: {remote_commit_id}",
                    remote_commit_id = remote_commit.id()
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
                        "Merge conflicts detected during pull. Please resolve conflicts and commit manually."
                    ));
                }

                // Create merge commit
                let tree_id = index.write_tree().context("Failed to write merge tree")?;
                let tree = self
                    .repo()
                    .find_tree(tree_id)
                    .context("Failed to find merge tree")?;

                let commit_message = format!("Merge branch '{remote_branch}' into {target_branch}");

                let merge_commit_id = self
                    .repo()
                    .commit(
                        Some("HEAD"),
                        &signature,
                        &signature,
                        &commit_message,
                        &tree,
                        &[&head_commit, &remote_commit],
                    )
                    .context("Failed to create merge commit")?;

                // Clean up merge state
                self.repo()
                    .cleanup_state()
                    .context("Failed to cleanup merge state")?;

                Ok(format!("Pull merge commit created: {merge_commit_id}"))
            } else {
                Err(anyhow::anyhow!(
                    "Unsupported merge analysis result during pull"
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{
        RepoAssertions, RepoTestOperations, create_test_bare_repo, create_test_repo,
    };

    #[test]
    fn fetch_works() {
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

        // Create and push a feature branch on remote side
        local_repo.create_and_checkout_branch("feature").unwrap();
        local_repo
            .add_file_and_commit("feature.txt", "feature content", "Add feature")
            .unwrap();
        local_repo.push("origin", "feature").unwrap();

        // Switch back to master
        local_repo.checkout_branch("master").unwrap();

        // Fetch specific branch (should update remote tracking)
        let result = local_repo.fetch("origin", Some("feature")).unwrap();
        assert!(result.contains("Fetched") || result.contains("up-to-date"));

        // Fetch all branches
        let result = local_repo.fetch("origin", None).unwrap();
        assert!(result.contains("Fetched") || result.contains("up-to-date"));

        // Test fetching from non-existent remote
        let result = local_repo.fetch("nonexistent", None);
        assert!(result.is_err());
    }

    #[test]
    fn pull_works() {
        let (_remote_dir, remote_repo) = create_test_bare_repo();

        // Setup local repository
        let (local_dir, local_repo) = create_test_repo();
        local_repo
            .add_file_and_commit("README.md", "initial", "Initial commit")
            .unwrap();

        // Add the remote repository and push
        local_repo.add_local_remote("origin", &remote_repo).unwrap();
        local_repo.push("origin", "master").unwrap();

        // Make more changes in the first repo and push them
        local_repo
            .add_file_and_commit("new_file.txt", "new content", "Add new file")
            .unwrap();
        local_repo.push("origin", "master").unwrap();

        // Reset the local repo to the previous commit to simulate being behind
        let commits = local_repo.list_commits().unwrap();
        assert!(commits.len() >= 2);
        let previous_commit_hash = &commits[1].hash; // Second commit (previous one)

        // Use git command to reset (since we don't have a reset method)
        std::process::Command::new("git")
            .args(["reset", "--hard", previous_commit_hash])
            .current_dir(local_dir.path())
            .output()
            .unwrap();

        // Pull changes in the first repo
        let result = local_repo.pull("origin", Some("master")).unwrap();
        assert!(result.contains("Fast-forward") || result.contains("up-to-date"));

        // Verify the new file exists
        local_repo.assert_file_exists("new_file.txt");

        // Test pulling when already up-to-date
        let result = local_repo.pull("origin", Some("master")).unwrap();
        assert_eq!(result, "Already up-to-date");

        // Test pulling from non-existent remote
        let result = local_repo.pull("nonexistent", None);
        assert!(result.is_err());
    }
}
