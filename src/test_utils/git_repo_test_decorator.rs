use crate::git::GitRepo;
use anyhow::{Context, Error};
use std::ops::Deref;
use std::result::Result::Ok;

/// Test decorator that enhances GitRepo with additional methods for testing
pub struct GitRepoTestDecorator {
    inner: GitRepo,
}

impl GitRepoTestDecorator {
    pub fn new(git_repo: GitRepo) -> Self {
        Self { inner: git_repo }
    }

    pub fn add_file(&self, filename: &str, content: &str) -> Result<&Self, Error> {
        let file_path = self.inner.path().join(filename);
        std::fs::write(file_path, content).unwrap();
        Ok(self)
    }

    /// Append a new line to an existing file
    pub fn append_to_file(&self, filename: &str, content: &str) -> Result<&Self, Error> {
        let file_path = self.inner.path().join(filename);

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

    pub fn add_file_and_commit(
        &self,
        filename: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<&Self, Error> {
        self.add_file(filename, content)?
            .add(&[filename])?
            .commit(commit_message)?;

        Ok(self)
    }

    /// Append content to an existing file and commit the changes
    pub fn append_to_file_and_commit(
        &self,
        filename: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<&Self, Error> {
        self.append_to_file(filename, content)?
            .add(&[filename])?
            .commit(commit_message)?;

        Ok(self)
    }

    /// Add a remote pointing to another local GitRepo
    pub fn add_local_remote(&self, name: &str, other_repo: &GitRepo) -> Result<(), Error> {
        let remote_path = other_repo
            .path()
            .to_str()
            .context("Failed to convert remote repository path to string")?;

        self.add_remote(name, remote_path)
    }

    // ===================== Assert functions ==================

    pub fn assert_commit_messages(&self, expected_messages: &[&str]) -> &Self {
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

    /// Assert that HEAD's symbolic target matches the expected value
    pub fn assert_head_symbolic_target(&self, expected_target: &str) -> &Self {
        match self.inner.get_head_symbolic_target() {
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

    pub fn assert_current_branch(&self, branch_name: &str) -> &Self {
        let expected_target = format!("refs/heads/{branch_name}");
        self.assert_head_symbolic_target(&expected_target);

        self
    }

    /// Assert that a file exists in the repository
    pub fn assert_file_exists(&self, filename: &str) -> &Self {
        let file_path = self.inner.path().join(filename);
        if !file_path.exists() {
            panic!("Expected file '{filename}' to exist at path: {file_path:?}");
        }
        self
    }

    /// Assert that a file does not exist in the repository
    pub fn assert_file_not_exists(&self, filename: &str) -> &Self {
        let file_path = self.inner.path().join(filename);
        if file_path.exists() {
            panic!("Expected file '{filename}' to not exist at path: {file_path:?}");
        }
        self
    }

    // ===================== Fluent wrappers for branch operations ==================

    /// Create and checkout branch (fluent wrapper)
    pub fn create_and_checkout_branch(&self, branch_name: &str) -> Result<&Self, Error> {
        self.inner.create_and_checkout_branch(branch_name)?;
        Ok(self)
    }

    /// Checkout branch (fluent wrapper)
    pub fn checkout_branch(&self, branch_name: &str) -> Result<&Self, Error> {
        self.inner.checkout_branch(branch_name)?;
        Ok(self)
    }

    /// Add files to staging (fluent wrapper)
    pub fn add(&self, pathspecs: &[&str]) -> Result<&Self, Error> {
        self.inner.add(pathspecs)?;
        Ok(self)
    }

    /// Commit staged changes (fluent wrapper)
    pub fn commit(&self, message: &str) -> Result<&Self, Error> {
        self.inner.commit(message)?;
        Ok(self)
    }

    /// Merge branch (fluent wrapper)
    pub fn merge(&self, branch_name: &str, message: Option<&str>) -> Result<&Self, Error> {
        self.inner.merge(branch_name, message)?;
        Ok(self)
    }
}

impl Deref for GitRepoTestDecorator {
    type Target = GitRepo;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {

    use crate::{git::GitRepo, test_utils::GitRepoTestDecorator};

    #[test]
    fn assert_commit_messages_works_correctly() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        repo.assert_commit_messages(&[]);

        let commit_message_1 = "Test commit 1";
        repo.add_file_and_commit("test_file_1.txt", "foo", commit_message_1)
            .unwrap();

        repo.assert_commit_messages(&[commit_message_1]);

        let commit_message_2 = "Test commit 2";
        repo.add_file_and_commit("test_file_2.txt", "foo", commit_message_2)
            .unwrap();

        repo.assert_commit_messages(&[commit_message_2, commit_message_1]);
    }

    #[test]
    fn add_local_remote_works() {
        let local_dir = assert_fs::TempDir::new().unwrap();
        let remote_dir = assert_fs::TempDir::new().unwrap();

        let local_repo = GitRepoTestDecorator::new(GitRepo::init(local_dir.path()).unwrap());
        let remote_repo = GitRepoTestDecorator::new(GitRepo::init(remote_dir.path()).unwrap());

        // Initially no remotes
        let remotes = local_repo.get_remotes().unwrap();
        assert_eq!(remotes.len(), 0);

        // Add repo2 as remote for repo1
        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Verify the remote was added with correct path
        let remotes = local_repo.get_remotes().unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].url, remote_dir.path().to_str().unwrap());
    }

    #[test]
    fn append_to_file_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        let filename = "test.txt";

        // Create initial file
        repo.add_file(filename, "line1").unwrap();

        // Append to file
        repo.append_to_file(filename, "line2").unwrap();
        repo.append_to_file(filename, "line3").unwrap();

        // Read file and verify content
        let file_path = path.join(filename);
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3");
    }

    #[test]
    fn append_to_nonexistent_file_fails() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        // Try to append to non-existent file
        let result = repo.append_to_file("nonexistent.txt", "content");
        assert!(result.is_err());
    }

    #[test]
    fn append_to_file_and_commit_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        let filename = "changelog.txt";

        // Create initial file and commit
        repo.add_file_and_commit(filename, "v1.0.0 - Initial release", "Initial changelog")
            .unwrap();

        // Append to file and commit
        repo.append_to_file_and_commit(filename, "v1.0.1 - Bug fixes", "Add v1.0.1 to changelog")
            .unwrap();
        repo.append_to_file_and_commit(
            filename,
            "v1.1.0 - New features",
            "Add v1.1.0 to changelog",
        )
        .unwrap();

        // Verify file content
        let file_path = path.join(filename);
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(
            content,
            "v1.0.0 - Initial release\nv1.0.1 - Bug fixes\nv1.1.0 - New features"
        );

        // Verify commits were created in correct order (newest first)
        repo.assert_commit_messages(&[
            "Add v1.1.0 to changelog",
            "Add v1.0.1 to changelog",
            "Initial changelog",
        ]);
    }
}
