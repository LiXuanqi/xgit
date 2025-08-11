use anyhow::{Context, Error};
use git2::Sort;

use crate::git::repository::core::{CommitInfo, GitRepo};

impl GitRepo {
    pub fn list_commits(&self) -> Result<Vec<CommitInfo>, Error> {
        let mut revwalk = self.repo().revwalk().context("Failed to create revwalk")?;

        revwalk
            .set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
            .context("Failed to set sorting")?;

        // Check if repository has any commits
        match self.repo().head() {
            Ok(_) => {
                revwalk.push_head().context("Failed to push HEAD")?;
            }
            Err(_) => {
                // No commits in repository, return empty vec
                return Ok(Vec::new());
            }
        }

        let mut commits = Vec::new();

        for oid in revwalk {
            let oid = oid.context("Failed to get commit OID")?;
            let commit = self
                .repo()
                .find_commit(oid)
                .context("Failed to find commit")?;

            commits.push(CommitInfo {
                hash: oid.to_string(),
                message: commit.message().unwrap_or("").to_string(),
            });
        }

        Ok(commits)
    }

    pub fn add(&self, pathspecs: &[&str]) -> Result<&Self, Error> {
        let mut index = self
            .repo()
            .index()
            .context("Failed to get repository index")?;

        index
            .add_all(pathspecs, git2::IndexAddOption::DEFAULT, None)
            .context("Failed to add files to index")?;

        index.write().context("Failed to write index")?;

        Ok(self)
    }

    pub fn commit(&self, message: &str) -> Result<String, Error> {
        let signature = self
            .create_signature()
            .context("Failed to create signature")?;

        let mut index = self
            .repo()
            .index()
            .context("Failed to get repository index")?;

        let tree_id = index
            .write_tree()
            .context("Failed to write tree from index")?;

        let tree = self
            .repo()
            .find_tree(tree_id)
            .context("Failed to find tree")?;

        // Get parent commit (if any)
        let parent_commit = match self.repo().head() {
            Ok(head) => {
                let target = head.target().context("Failed to get HEAD target")?;
                Some(
                    self.repo()
                        .find_commit(target)
                        .context("Failed to find parent commit")?,
                )
            }
            Err(_) => None, // First commit, no parent
        };

        let parents: Vec<_> = parent_commit.iter().collect();

        let commit_id = self
            .repo()
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .context("Failed to create commit")?;

        Ok(commit_id.to_string())
    }

    pub fn get_branch_commit_info(&self, branch: &str) -> Result<String, Error> {
        // Get the commit that the branch points to
        let branch_ref = format!("refs/heads/{branch}");
        let reference = self
            .repo()
            .find_reference(&branch_ref)
            .context(format!("Failed to find branch reference: {branch_ref}"))?;
        let commit_oid = reference
            .target()
            .ok_or_else(|| anyhow::anyhow!("Branch reference has no target"))?;
        let commit = self
            .repo()
            .find_commit(commit_oid)
            .context("Failed to find commit")?;

        // Get short hash (first 7 characters)
        let short_hash = commit.id().to_string()[..7].to_string();

        // Get commit message (first line only)
        let message = commit.message().unwrap_or("No commit message");
        let first_line = message.lines().next().unwrap_or("No commit message");

        Ok(format!("{short_hash} {first_line}"))
    }

    /// Check if there are any staged files in the index
    pub fn has_staged_changes(&self) -> Result<bool, Error> {
        let mut index = self
            .repo()
            .index()
            .context("Failed to get repository index")?;

        // If repository has no commits yet, any files in index are staged
        if self.repo().head().is_err() {
            return Ok(!index.is_empty());
        }

        // Compare index tree with HEAD tree
        let head = self.repo().head().context("Failed to get HEAD")?;
        let head_commit = head
            .peel_to_commit()
            .context("Failed to peel HEAD to commit")?;
        let head_tree = head_commit.tree().context("Failed to get HEAD tree")?;

        let index_tree_id = index.write_tree().context("Failed to write index tree")?;

        Ok(head_tree.id() != index_tree_id)
    }

    /// Get diff object of staged changes
    pub fn get_staged_diff(&self) -> Result<git2::Diff<'_>, Error> {
        let index = self
            .repo()
            .index()
            .context("Failed to get repository index")?;

        let diff = if self.repo().head().is_err() {
            // No commits yet, diff against empty tree
            let empty_tree = self
                .repo()
                .treebuilder(None)?
                .write()
                .context("Failed to create empty tree")?;
            let empty_tree = self.repo().find_tree(empty_tree)?;

            self.repo()
                .diff_tree_to_index(Some(&empty_tree), Some(&index), None)
                .context("Failed to create diff from empty tree to index")?
        } else {
            // Compare HEAD tree with index
            let head = self.repo().head().context("Failed to get HEAD")?;
            let head_commit = head
                .peel_to_commit()
                .context("Failed to peel HEAD to commit")?;
            let head_tree = head_commit.tree().context("Failed to get HEAD tree")?;

            self.repo()
                .diff_tree_to_index(Some(&head_tree), Some(&index), None)
                .context("Failed to create diff from HEAD to index")?
        };

        Ok(diff)
    }

    /// Convert a diff to string format
    pub fn diff_to_string(&self, diff: &git2::Diff) -> Result<String, Error> {
        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            match line.origin() {
                '+' | '-' | ' ' | 'F' | 'H' => {
                    diff_text.push(line.origin());
                    if let Ok(content) = std::str::from_utf8(line.content()) {
                        diff_text.push_str(content);
                    }
                }
                _ => {
                    // Include other lines like file headers without prefix
                    if let Ok(content) = std::str::from_utf8(line.content()) {
                        diff_text.push_str(content);
                    }
                }
            }
            true
        })?;

        Ok(diff_text)
    }

    /// Generate diff string of staged changes (convenience method)
    pub fn diff_staged(&self) -> Result<String, Error> {
        let diff = self.get_staged_diff()?;
        self.diff_to_string(&diff)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{create_test_repo, RepoAssertions, RepoTestOperations};

    #[test]
    fn list_commits_works_in_repo_without_any_commit() {
        let (_temp_dir, repo) = create_test_repo();

        let commits = repo.list_commits().unwrap();

        assert_eq!(commits.len(), 0);
    }

    #[test]
    fn list_commits_works() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        let commits = repo.list_commits().unwrap();
        assert_eq!(commits.len(), 0);

        repo.add_file_and_commit("test_file_1.txt", "foo", "Test commit 1")?
            .add_file_and_commit("test_file_2.txt", "foo", "Test commit 2")?
            .add_file_and_commit("test_file_3.txt", "foo", "Test commit 3")?
            .assert_commit_messages(&["Test commit 3", "Test commit 2", "Test commit 1"]);

        let commits = repo.list_commits().unwrap();
        assert_eq!(commits.len(), 3);

        Ok(())
    }

    #[test]
    fn add_works_for_single_file_path() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        let file_name = "test_file.txt";
        repo.add_file(file_name, "foo")?.add(&[file_name])?;

        // Verify the file is staged
        let index = repo.repo().index().unwrap();
        let entry = index.get_path(std::path::Path::new(file_name), 0);
        assert!(entry.is_some(), "File should be in the index after adding");
        Ok(())
    }

    #[test]
    fn add_works_for_glob_patterns() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        repo.add_file("test_file_1.txt", "foo")?
            .add_file("test_file_2.txt", "foo")?
            .add_file("test_file_non_text.rs", "foo")?
            .add(&["*.txt"])?;

        // Verify the file is staged
        let index = repo.repo().index().unwrap();
        assert_eq!(index.len(), 2);
        Ok(())
    }

    #[test]
    fn add_works_for_all_files() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        repo.add_file("test_file_1.txt", "foo")?
            .add_file("test_file_2.txt", "foo")?
            .add_file("test_file_non_text.rs", "foo")?
            .add(&["."])?;

        // Verify the file is staged
        let index = repo.repo().index().unwrap();
        assert_eq!(index.len(), 3);
        Ok(())
    }

    #[test]
    fn has_staged_changes_works() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        // Initially no staged changes
        assert!(!repo.has_staged_changes().unwrap());

        // Add a file without staging
        repo.add_file("test.txt", "content")?;
        assert!(!repo.has_staged_changes().unwrap());

        // Stage the file
        repo.add(&["test.txt"])?;
        assert!(repo.has_staged_changes().unwrap());

        // Commit the file
        repo.commit("Initial commit")?;
        assert!(!repo.has_staged_changes().unwrap());

        // Add another file and stage it
        repo.add_file("test2.txt", "content2")?
            .add(&["test2.txt"])?;
        assert!(repo.has_staged_changes().unwrap());
        Ok(())
    }

    #[test]
    fn diff_staged_works() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        // No staged changes initially
        let diff = repo.diff_staged().unwrap();
        assert!(diff.is_empty());

        // Add and stage a file
        repo.add_file("test.txt", "Hello World")?
            .add(&["test.txt"])?;

        let diff = repo.diff_staged().unwrap();
        assert!(diff.contains("Hello World"));
        assert!(diff.contains("+Hello World"));

        // Commit the file
        repo.commit("Add test file")?;

        // No staged changes after commit
        let diff = repo.diff_staged().unwrap();
        assert!(diff.is_empty());

        // Modify and stage the file
        repo.add_file("test.txt", "Hello World\nSecond line")?
            .add(&["test.txt"])?;

        let diff = repo.diff_staged().unwrap();
        assert!(diff.contains("+Second line"));
        Ok(())
    }

    #[test]
    fn get_staged_diff_and_diff_to_string_work() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        // Add and stage a file
        repo.add_file("test.txt", "Hello World")?
            .add(&["test.txt"])?;

        // Test get_staged_diff returns a Diff object
        let diff_obj = repo.get_staged_diff().unwrap();
        assert_eq!(diff_obj.deltas().len(), 1);

        // Test diff_to_string converts the Diff to string
        let diff_string = repo.diff_to_string(&diff_obj).unwrap();
        assert!(diff_string.contains("Hello World"));

        // Should be same as calling diff_staged directly
        let direct_diff = repo.diff_staged().unwrap();
        assert_eq!(diff_string, direct_diff);
        Ok(())
    }

    #[test]
    fn get_branch_commit_info_works() -> Result<(), Box<dyn std::error::Error>> {
        let (_temp_dir, repo) = create_test_repo();

        // Add initial commit
        repo.add_file_and_commit("README.md", "initial", "Initial commit")?;

        let commit_info = repo.get_branch_commit_info("master").unwrap();
        assert!(commit_info.contains("Initial commit"));
        assert!(commit_info.len() > 7); // Should have short hash + message

        // Test with feature branch
        repo.create_and_checkout_branch("feature")?
            .add_file_and_commit("feature.txt", "feature content", "Add feature")?;

        let feature_commit_info = repo.get_branch_commit_info("feature").unwrap();
        assert!(feature_commit_info.contains("Add feature"));

        // Test with non-existent branch
        let result = repo.get_branch_commit_info("nonexistent");
        assert!(result.is_err());
        Ok(())
    }
}
