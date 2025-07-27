use crate::git::GitRepo;
use anyhow::{Context, Error};

/// Create a new temporary repository for testing with user config set up
#[cfg(test)]
pub fn create_test_repo() -> (assert_fs::TempDir, GitRepo) {
    let temp_dir = assert_fs::TempDir::new().unwrap();
    let path = temp_dir.path();
    let repo = GitRepo::init(path).unwrap();
    repo.set_user_config("Test User", "test@example.com")
        .unwrap();
    (temp_dir, repo)
}

/// Create a new temporary bare repository for testing
#[cfg(test)]
pub fn create_test_bare_repo() -> (assert_fs::TempDir, GitRepo) {
    let temp_dir = assert_fs::TempDir::new().unwrap();
    let path = temp_dir.path();
    let repo = GitRepo::init_bare(path).unwrap();
    repo.set_user_config("Test User", "test@example.com")
        .unwrap();
    (temp_dir, repo)
}

/// Test-only trait that adds assertion methods to GitRepo
#[cfg(test)]
pub trait RepoAssertions {
    /// Assert that HEAD's symbolic target matches the expected value
    fn assert_head_symbolic_target(&self, expected_target: &str) -> &Self;

    /// Assert that the current branch matches the expected branch name
    fn assert_current_branch(&self, branch_name: &str) -> &Self;

    /// Assert that a file exists in the repository
    fn assert_file_exists(&self, filename: &str) -> &Self;

    /// Assert that a file does not exist in the repository
    fn assert_file_not_exists(&self, filename: &str) -> &Self;

    /// Assert that commit messages match the expected order (newest first)
    fn assert_commit_messages(&self, expected_messages: &[&str]) -> &Self;
}

/// Test-only trait that adds test helper operations to GitRepo
#[cfg(test)]
pub trait RepoTestOperations {
    /// Add a file with content (fluent)
    fn add_file(&self, filename: &str, content: &str) -> Result<&Self, Error>;

    /// Append content to an existing file (fluent)
    fn append_to_file(&self, filename: &str, content: &str) -> Result<&Self, Error>;

    /// Add a file and commit in one operation (fluent)
    fn add_file_and_commit(
        &self,
        filename: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<&Self, Error>;

    /// Append to file and commit in one operation (fluent)
    fn append_to_file_and_commit(
        &self,
        filename: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<&Self, Error>;

    /// Add a remote pointing to another local GitRepo
    fn add_local_remote(&self, name: &str, other_repo: &GitRepo) -> Result<(), Error>;

    /// Commit staged changes (fluent wrapper that ignores return value)
    fn commit_fluent(&self, message: &str) -> Result<&Self, Error>;

    /// Merge branch (fluent wrapper that ignores return value)
    fn merge_fluent(&self, branch_name: &str, message: Option<&str>) -> Result<&Self, Error>;
}

#[cfg(test)]
impl RepoAssertions for GitRepo {
    fn assert_head_symbolic_target(&self, expected_target: &str) -> &Self {
        match self.get_head_symbolic_target() {
            Ok(actual_target) => {
                if actual_target != expected_target {
                    panic!(
                        "HEAD symbolic target mismatch. Expected: '{expected_target}', Found: '{actual_target}'"
                    );
                }
            }
            Err(e) => {
                panic!("Failed to get HEAD symbolic target: {e}");
            }
        }
        self
    }

    fn assert_current_branch(&self, branch_name: &str) -> &Self {
        let expected_target = format!("refs/heads/{branch_name}");
        self.assert_head_symbolic_target(&expected_target);
        self
    }

    fn assert_file_exists(&self, filename: &str) -> &Self {
        let file_path = self.path().join(filename);
        if !file_path.exists() {
            panic!("Expected file '{filename}' to exist at path: {file_path:?}");
        }
        self
    }

    fn assert_file_not_exists(&self, filename: &str) -> &Self {
        let file_path = self.path().join(filename);
        if file_path.exists() {
            panic!("Expected file '{filename}' to not exist at path: {file_path:?}");
        }
        self
    }

    fn assert_commit_messages(&self, expected_messages: &[&str]) -> &Self {
        let commits = self.list_commits().unwrap_or_else(|_| Vec::new());

        if commits.len() != expected_messages.len() {
            panic!(
                "Expected {} commits, but found {}. Commits: {:?}",
                expected_messages.len(),
                commits.len(),
                commits.iter().map(|c| &c.message).collect::<Vec<_>>()
            );
        }

        for (i, (commit, expected)) in commits.iter().zip(expected_messages.iter()).enumerate() {
            if commit.message != *expected {
                panic!(
                    "Commit {} message mismatch. Expected: '{}', Found: '{}'",
                    i, expected, commit.message
                );
            }
        }

        self
    }
}

#[cfg(test)]
impl RepoTestOperations for GitRepo {
    fn add_file(&self, filename: &str, content: &str) -> Result<&Self, Error> {
        let file_path = self.path().join(filename);
        std::fs::write(file_path, content)?;
        Ok(self)
    }

    fn append_to_file(&self, filename: &str, content: &str) -> Result<&Self, Error> {
        let file_path = self.path().join(filename);

        // Read existing content
        let mut existing_content = std::fs::read_to_string(&file_path)
            .context(format!("Failed to read existing file '{filename}'"))?;

        // Add newline if file doesn't end with one
        if !existing_content.is_empty() && !existing_content.ends_with('\n') {
            existing_content.push('\n');
        }

        // Append new content
        existing_content.push_str(content);

        // Write back to file
        std::fs::write(&file_path, existing_content)
            .context(format!("Failed to write to file '{filename}'"))?;

        Ok(self)
    }

    fn add_file_and_commit(
        &self,
        filename: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<&Self, Error> {
        self.add_file(filename, content)?
            .add(&[filename])?
            .commit_fluent(commit_message)?;

        Ok(self)
    }

    fn append_to_file_and_commit(
        &self,
        filename: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<&Self, Error> {
        self.append_to_file(filename, content)?
            .add(&[filename])?
            .commit_fluent(commit_message)?;

        Ok(self)
    }

    fn add_local_remote(&self, name: &str, other_repo: &GitRepo) -> Result<(), Error> {
        let remote_path = other_repo
            .path()
            .to_str()
            .context("Failed to convert remote repository path to string")?;

        self.add_remote(name, remote_path)
    }

    fn commit_fluent(&self, message: &str) -> Result<&Self, Error> {
        self.commit(message)?;
        Ok(self)
    }

    fn merge_fluent(&self, branch_name: &str, message: Option<&str>) -> Result<&Self, Error> {
        self.merge(branch_name, message)?;
        Ok(self)
    }
}
