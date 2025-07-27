use anyhow::{Context, Error};

use crate::git::repository::core::{GitRepo, RemoteInfo};

impl GitRepo {
    /// Add a remote repository
    pub fn add_remote(&self, name: &str, url: &str) -> Result<(), Error> {
        self.repo()
            .remote(name, url)
            .context(format!("Failed to add remote '{name}' with URL '{url}'"))?;

        Ok(())
    }

    /// Set the URL of an existing remote
    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<(), Error> {
        self.repo()
            .remote_set_url(name, url)
            .context(format!("Failed to set URL for remote '{name}'"))?;

        Ok(())
    }

    /// List all remotes with their URLs
    pub fn get_remotes(&self) -> Result<Vec<RemoteInfo>, Error> {
        let remotes = self
            .repo()
            .remotes()
            .context("Failed to get remotes list")?;

        let mut remote_infos = Vec::new();
        for i in 0..remotes.len() {
            if let Some(name) = remotes.get(i) {
                let remote = self
                    .repo()
                    .find_remote(name)
                    .context(format!("Failed to find remote '{name}'"))?;

                let url = remote.url().unwrap_or("<no url>").to_string();

                remote_infos.push(RemoteInfo {
                    name: name.to_string(),
                    url,
                });
            }
        }

        Ok(remote_infos)
    }

    /// List all remote names only (for backward compatibility)
    pub fn get_remote_names(&self) -> Result<Vec<String>, Error> {
        let remotes = self.get_remotes()?;
        Ok(remotes.into_iter().map(|r| r.name).collect())
    }

    /// Get the URL of a specific remote
    pub fn get_remote_url(&self, name: &str) -> Result<String, Error> {
        let remote = self
            .repo()
            .find_remote(name)
            .context(format!("Failed to find remote '{name}'"))?;

        let url = remote
            .url()
            .ok_or_else(|| anyhow::anyhow!("Remote '{name}' has no URL"))?;

        Ok(url.to_string())
    }

    /// Push current branch to remote (equivalent to `git push <remote> <branch>`)
    ///
    /// # Arguments
    /// * `remote_name` - The name of the remote (e.g., "origin")
    /// * `branch_name` - The name of the branch to push (e.g., "main", "master")
    pub fn push(&self, remote_name: &str, branch_name: &str) -> Result<(), Error> {
        let mut remote = self
            .repo()
            .find_remote(remote_name)
            .context(format!("Failed to find remote '{remote_name}'"))?;

        let refspec = format!("refs/heads/{branch_name}:refs/heads/{branch_name}");

        remote.push(&[&refspec], None).context(format!(
            "Failed to push branch '{branch_name}' to remote '{remote_name}'"
        ))?;

        Ok(())
    }

    /// Push current HEAD branch to remote (equivalent to `git push <remote>`)
    ///
    /// # Arguments
    /// * `remote_name` - The name of the remote (e.g., "origin")
    pub fn push_current_branch(&self, remote_name: &str) -> Result<(), Error> {
        // Get current branch name from HEAD
        let head_target = self
            .get_head_symbolic_target()
            .context("Failed to get current branch from HEAD")?;

        // Extract branch name from "refs/heads/branch_name"
        let branch_name = head_target
            .strip_prefix("refs/heads/")
            .ok_or_else(|| anyhow::anyhow!("HEAD is not pointing to a branch"))?;

        self.push(remote_name, branch_name)
    }

    /// Push current branch to origin remote (equivalent to `git push`)
    pub fn push_to_origin(&self) -> Result<(), Error> {
        self.push_current_branch("origin")
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        git::{GitRepo, repository::core::RemoteInfo},
        test_utils::{RepoTestOperations, create_test_bare_repo, create_test_repo},
    };

    #[test]
    fn add_remote_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepo::init(path).unwrap();

        let remotes = repo.get_remotes().unwrap();
        assert_eq!(remotes.len(), 0);

        repo.add_remote("origin", "https://url1").unwrap();
        let remotes = repo.get_remotes().unwrap();

        assert_eq!(
            remotes,
            vec![RemoteInfo {
                name: "origin".to_string(),
                url: "https://url1".to_string()
            }]
        );
    }

    #[test]
    fn set_remote_url_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepo::init(path).unwrap();

        let remotes = repo.get_remotes().unwrap();
        assert_eq!(remotes.len(), 0);

        repo.add_remote("origin", "https://url1").unwrap();
        let remotes = repo.get_remotes().unwrap();

        assert_eq!(
            remotes,
            vec![RemoteInfo {
                name: "origin".to_string(),
                url: "https://url1".to_string()
            }]
        );

        repo.set_remote_url("origin", "https://url2").unwrap();
        let remotes = repo.get_remotes().unwrap();

        assert_eq!(
            remotes,
            vec![RemoteInfo {
                name: "origin".to_string(),
                url: "https://url2".to_string()
            }]
        );
    }

    #[test]
    fn get_remotes_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepo::init(path).unwrap();

        let remotes = repo.get_remotes().unwrap();
        assert_eq!(remotes.len(), 0);

        repo.add_remote("origin", "https://url1").unwrap();
        repo.add_remote("origin_2", "https://url2").unwrap();
        let remotes = repo.get_remotes().unwrap();

        assert_eq!(
            remotes,
            vec![
                RemoteInfo {
                    name: "origin".to_string(),
                    url: "https://url1".to_string()
                },
                RemoteInfo {
                    name: "origin_2".to_string(),
                    url: "https://url2".to_string()
                }
            ]
        );
    }

    #[test]
    fn get_remote_names_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepo::init(path).unwrap();

        let remote_names = repo.get_remote_names().unwrap();
        assert_eq!(remote_names.len(), 0);

        repo.add_remote("origin", "https://url1").unwrap();
        repo.add_remote("origin_2", "https://url2").unwrap();
        let remote_names = repo.get_remote_names().unwrap();

        assert_eq!(remote_names, vec!["origin", "origin_2"]);
    }

    #[test]
    fn push_works() {
        let (_remote_dir, remote_repo) = create_test_bare_repo();

        // Verify remote is empty
        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches.len(), 0);

        // Setup local repository
        let (_local_dir, local_repo) = create_test_repo();
        local_repo
            .add_file_and_commit("test.txt", "content", "Initial commit")
            .unwrap();

        // Add the remote repository
        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push to remote
        local_repo.push("origin", "master").unwrap();

        // Verify the remote now has the branch
        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches, vec!["master"]);
    }

    #[test]
    fn push_current_branch_works() {
        let (_remote_dir, remote_repo) = create_test_bare_repo();

        // Setup local repository
        let (_local_dir, local_repo) = create_test_repo();
        local_repo
            .add_file_and_commit("test.txt", "content", "Initial commit")
            .unwrap();

        // Create and checkout a feature branch
        local_repo
            .create_and_checkout_branch("feature_branch")
            .unwrap();
        local_repo
            .add_file_and_commit("feature.txt", "feature content", "Feature commit")
            .unwrap();

        // Add the remote repository
        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push current branch (should be feature_branch)
        local_repo.push_current_branch("origin").unwrap();

        // Verify the remote now has the feature branch
        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches, vec!["feature_branch"]);
    }

    #[test]
    fn push_to_origin_works() {
        let (_remote_dir, remote_repo) = create_test_bare_repo();

        // Setup local repository
        let (_local_dir, local_repo) = create_test_repo();
        local_repo
            .add_file_and_commit("test.txt", "content", "Initial commit")
            .unwrap();

        // Create and checkout a feature branch
        local_repo
            .create_and_checkout_branch("feature_branch")
            .unwrap();
        local_repo
            .add_file_and_commit("feature.txt", "feature content", "Feature commit")
            .unwrap();

        // Add the remote repository as origin
        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push to origin (should push current branch)
        local_repo.push_to_origin().unwrap();

        // Verify the remote now has the feature branch
        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches, vec!["feature_branch"]);
    }
}
