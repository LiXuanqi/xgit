use anyhow::{Context, Error};
use git2::BranchType;

use crate::git::repository::core::GitRepo;

impl GitRepo {
    pub fn get_all_branches(&self) -> Result<Vec<String>, Error> {
        let mut branches = Vec::new();

        let branch_iter = self.repo().branches(Some(BranchType::Local))?;

        for branch in branch_iter {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                branches.push(name.to_string());
            }
        }

        Ok(branches)
    }

    /// Create a new branch from the current HEAD and switch to it
    pub fn create_and_checkout_branch(&self, branch_name: &str) -> Result<(), Error> {
        match self.repo().head() {
            Ok(head) => {
                // Repository has commits, create branch from HEAD
                let target_commit = head.target().context("Failed to get HEAD target")?;

                let commit = self
                    .repo()
                    .find_commit(target_commit)
                    .context("Failed to find HEAD commit")?;

                self.repo()
                    .branch(branch_name, &commit, false)
                    .context("Failed to create branch")?;

                // Switch to the new branch
                self.repo()
                    .set_head(&format!("refs/heads/{branch_name}"))
                    .context("Failed to set HEAD to new branch")?;
            }
            Err(_) => {
                // Repository has no commits, just switch HEAD to point to the new branch
                // This creates an unborn branch
                self.repo()
                    .set_head(&format!("refs/heads/{branch_name}"))
                    .context("Failed to set HEAD to new branch")?;
            }
        }

        Ok(())
    }

    pub fn checkout_branch(&self, branch_name: &str) -> Result<(), Error> {
        // Get the branch reference
        let branch_ref = format!("refs/heads/{branch_name}");
        let obj = self.repo().revparse_single(&branch_ref)?;

        // Checkout the branch
        if !self.is_bare() {
            self.repo().checkout_tree(&obj, None)?;
        }

        // Set HEAD to point to the branch
        self.repo().set_head(&branch_ref)?;

        Ok(())
    }

    pub fn get_head_symbolic_target(&self) -> Result<String, Error> {
        let head_ref = self
            .repo()
            .find_reference("HEAD")
            .context("Failed to find HEAD reference")?;

        match head_ref.symbolic_target() {
            Some(target) => Ok(target.to_string()),
            None => Err(anyhow::anyhow!("HEAD is not a symbolic reference")),
        }
    }

    /// Get the current branch name
    pub fn get_current_branch(&self) -> Result<String, Error> {
        let head_target = self
            .get_head_symbolic_target()
            .context("Failed to get current branch from HEAD")?;

        // Extract branch name from "refs/heads/branch_name"
        let branch_name = head_target
            .strip_prefix("refs/heads/")
            .ok_or_else(|| anyhow::anyhow!("HEAD is not pointing to a branch"))?;

        Ok(branch_name.to_string())
    }

    /// Check if a specific branch is merged to main
    pub fn is_branch_merged_to_main(&self, branch_name: &str) -> Result<bool, Error> {
        let branch_ref = self
            .repo()
            .find_reference(&format!("refs/heads/{branch_name}"))
            .context("Failed to find branch reference")?;
        let branch_oid = branch_ref.target().context("Failed to get branch target")?;

        let main_ref = self
            .repo()
            .find_reference("refs/heads/main")
            .or_else(|_| self.repo().find_reference("refs/heads/master"))
            .context("Failed to find main/master branch")?;
        let main_oid = main_ref.target().context("Failed to get main target")?;

        let merge_base = self
            .repo()
            .merge_base(branch_oid, main_oid)
            .context("Failed to find merge base")?;

        Ok(merge_base == branch_oid)
    }
}

#[cfg(test)]
mod tests {
    use crate::{git::GitRepo, test_utils::GitRepoTestDecorator};

    #[test]
    fn create_branch_and_get_all_branches_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());
        repo.add_file_and_commit("test_file_1.txt", "foo", "Test commit 1")
            .unwrap();

        let branch_1 = "foo_branch";
        let branch_2 = "bar_branch";
        repo.create_and_checkout_branch(branch_1).unwrap();

        repo.assert_current_branch(branch_1);
        repo.create_and_checkout_branch(branch_2).unwrap();

        repo.assert_current_branch(branch_2);

        let mut actual = repo.get_all_branches().unwrap();
        let mut expected = vec!["master", branch_1, branch_2];

        actual.sort();
        expected.sort();

        assert_eq!(actual, expected);
    }

    #[test]
    fn create_branch_works_when_no_commit() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        let branch = "bar_branch";
        repo.create_and_checkout_branch(branch).unwrap();

        let actual = repo.get_all_branches().unwrap();
        assert_eq!(actual.len(), 0);

        // Check current branch changed
        repo.assert_current_branch(branch);

        // After commit, branch should appear
        repo.add_file_and_commit("test.txt", "content", "Initial commit")
            .unwrap();

        let actual = repo.get_all_branches().unwrap();
        assert_eq!(actual, vec![branch]);
    }

    #[test]
    fn checkout_branch_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        repo.add_file_and_commit("test_file_1.txt", "foo", "Test commit 1")
            .unwrap();

        // Create a feature branch
        repo.create_and_checkout_branch("feature-branch").unwrap();
        repo.assert_current_branch("feature-branch");

        // Add a commit to feature branch
        repo.add_file_and_commit("feature.txt", "feature content", "Feature commit")
            .unwrap();

        // Switch back to master
        repo.checkout_branch("master").unwrap();
        repo.assert_current_branch("master");

        // Feature file should not exist on master
        repo.assert_file_not_exists("feature.txt");

        // Switch back to feature branch
        repo.checkout_branch("feature-branch").unwrap();
        repo.assert_current_branch("feature-branch");

        // Feature file should exist on feature branch
        repo.assert_file_exists("feature.txt");
    }

    #[test]
    fn get_current_branch_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        // Add initial commit so branches work properly
        repo.add_file_and_commit("README.md", "initial", "Initial commit")
            .unwrap();

        // Initially on master
        let current_branch = repo.get_current_branch().unwrap();
        assert_eq!(current_branch, "master");

        // Create and switch to feature branch
        repo.create_and_checkout_branch("feature-branch").unwrap();

        let current_branch = repo.get_current_branch().unwrap();
        assert_eq!(current_branch, "feature-branch");

        // Switch back to master
        repo.checkout_branch("master").unwrap();

        let current_branch = repo.get_current_branch().unwrap();
        assert_eq!(current_branch, "master");
    }

    #[test]
    fn is_branch_merged_to_main_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        // Create initial commit on master
        repo.add_file_and_commit("README.md", "initial", "Initial commit")
            .unwrap();

        // Create feature branch
        repo.create_and_checkout_branch("feature-branch").unwrap();
        repo.add_file_and_commit("feature.txt", "feature content", "Feature commit")
            .unwrap();

        // Feature branch should not be merged to master yet
        assert!(!repo.is_branch_merged_to_main("feature-branch").unwrap());

        // Switch back to master and merge feature branch
        repo.checkout_branch("master").unwrap();
        repo.merge("feature-branch", None).unwrap();

        // Now feature branch should be merged to master
        assert!(repo.is_branch_merged_to_main("feature-branch").unwrap());
    }
}
