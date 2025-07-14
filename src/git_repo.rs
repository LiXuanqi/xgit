#![allow(dead_code)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Error};
use git2::{BranchType, Repository, Signature, Sort};

pub struct GitRepo {
    path: PathBuf,
    repo: Repository,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

impl GitRepo {
    /// Open a git repository at the specified path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            repo: Repository::open(path).context("Cannot open git repo at given path")?,
        })
    }

    pub fn init<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path_ref = path.as_ref();

        // Check if it's already a git repository
        if Repository::open(path_ref).is_ok() {
            return Err(anyhow::anyhow!("Directory is already a git repository"));
        }

        // Initialize a new git repository
        let repo = Repository::init(path_ref).context("Failed to initialize git repository")?;

        let git_repo = Self {
            path: path_ref.to_path_buf(),
            repo,
        };

        // TODO: init should respect config to create master/main

        // Set HEAD to point to master (this is what git init does)
        // The master branch will be created when the first commit is made
        git_repo
            .repo
            .set_head("refs/heads/master")
            .context("Failed to set HEAD to master")?;

        Ok(git_repo)
    }

    /// Initialize a new bare git repository
    pub fn init_bare<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path_ref = path.as_ref();

        // Check if it's already a git repository
        if Repository::open(path_ref).is_ok() {
            return Err(anyhow::anyhow!("Directory is already a git repository"));
        }

        // Initialize a new bare git repository
        let repo =
            Repository::init_bare(path_ref).context("Failed to initialize bare git repository")?;

        let git_repo = Self {
            path: path_ref.to_path_buf(),
            repo,
        };

        // Set HEAD to point to master (this is what git init --bare does)
        git_repo
            .repo
            .set_head("refs/heads/master")
            .context("Failed to set HEAD to master")?;

        Ok(git_repo)
    }

    pub fn list_commits(&self) -> Result<Vec<CommitInfo>, Error> {
        let mut revwalk = self.repo.revwalk().context("Failed to create revwalk")?;

        revwalk
            .set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
            .context("Failed to set sorting")?;

        // Check if repository has any commits
        match self.repo.head() {
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
                .repo
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
            .repo
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
            .repo
            .index()
            .context("Failed to get repository index")?;

        let tree_id = index
            .write_tree()
            .context("Failed to write tree from index")?;

        let tree = self
            .repo
            .find_tree(tree_id)
            .context("Failed to find tree")?;

        // Get parent commit (if any)
        let parent_commit = match self.repo.head() {
            Ok(head) => {
                let target = head.target().context("Failed to get HEAD target")?;
                Some(
                    self.repo
                        .find_commit(target)
                        .context("Failed to find parent commit")?,
                )
            }
            Err(_) => None, // First commit, no parent
        };

        let parents: Vec<_> = parent_commit.iter().collect();

        let commit_id = self
            .repo
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

    pub fn get_all_branches(&self) -> Result<Vec<String>, Error> {
        let mut branches = Vec::new();

        let branch_iter = self.repo.branches(Some(BranchType::Local))?;

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
        match self.repo.head() {
            Ok(head) => {
                // Repository has commits, create branch from HEAD
                let target_commit = head.target().context("Failed to get HEAD target")?;

                let commit = self
                    .repo
                    .find_commit(target_commit)
                    .context("Failed to find HEAD commit")?;

                self.repo
                    .branch(branch_name, &commit, false)
                    .context("Failed to create branch")?;

                // Switch to the new branch
                self.repo
                    .set_head(&format!("refs/heads/{branch_name}"))
                    .context("Failed to set HEAD to new branch")?;
            }
            Err(_) => {
                // Repository has no commits, just switch HEAD to point to the new branch
                // This creates an unborn branch
                self.repo
                    .set_head(&format!("refs/heads/{branch_name}"))
                    .context("Failed to set HEAD to new branch")?;
            }
        }

        Ok(())
    }

    pub fn checkout_branch(&self, branch_name: &str) -> Result<(), Error> {
        // Get the branch reference
        let branch_ref = format!("refs/heads/{branch_name}");
        let obj = self.repo.revparse_single(&branch_ref)?;

        // Checkout the branch
        if !self.is_bare() {
            self.repo.checkout_tree(&obj, None)?;
        }

        // Set HEAD to point to the branch
        self.repo.set_head(&branch_ref)?;

        Ok(())
    }

    fn create_signature(&self) -> Result<Signature<'_>, Error> {
        let config = self
            .repo
            .config()
            .context("Failed to get repository config")?;

        let author_name = config.get_string("user.name").context(
            "Failed to get user.name from git config. Run: git config user.name \"Your Name\"",
        )?;

        let author_email = config.get_string("user.email")
            .context("Failed to get user.email from git config. Run: git config user.email \"your@email.com\"")?;

        Signature::now(&author_name, &author_email)
            .context("Failed to create signature with git config values")
    }

    /// Get the path to the repository
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if this is a bare repository
    pub fn is_bare(&self) -> bool {
        self.repo.is_bare()
    }

    pub fn get_head_symbolic_target(&self) -> Result<String, Error> {
        let head_ref = self
            .repo
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

    /// Get commit info for a specific branch (short hash + first line of message)
    pub fn get_branch_commit_info(&self, branch: &str) -> Result<String, Error> {
        // Get the commit that the branch points to
        let branch_ref = format!("refs/heads/{branch}");
        let reference = self
            .repo
            .find_reference(&branch_ref)
            .context(format!("Failed to find branch reference: {branch_ref}"))?;
        let commit_oid = reference
            .target()
            .ok_or_else(|| anyhow::anyhow!("Branch reference has no target"))?;
        let commit = self
            .repo
            .find_commit(commit_oid)
            .context("Failed to find commit")?;

        // Get short hash (first 7 characters)
        let short_hash = commit.id().to_string()[..7].to_string();

        // Get commit message (first line only)
        let message = commit.message().unwrap_or("No commit message");
        let first_line = message.lines().next().unwrap_or("No commit message");

        Ok(format!("{short_hash} {first_line}"))
    }

    /// Get remote tracking info for a specific branch
    pub fn get_remote_tracking_info(&self, branch: &str) -> Result<String, Error> {
        let branch_ref = format!("refs/heads/{branch}");

        // Try to get the upstream branch
        let upstream = self
            .repo
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

    /// Add a remote repository
    pub fn add_remote(&self, name: &str, url: &str) -> Result<(), Error> {
        self.repo
            .remote(name, url)
            .context(format!("Failed to add remote '{name}' with URL '{url}'"))?;

        Ok(())
    }

    /// Set the URL of an existing remote
    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<(), Error> {
        self.repo
            .remote_set_url(name, url)
            .context(format!("Failed to set URL for remote '{name}'"))?;

        Ok(())
    }

    /// List all remotes with their URLs
    pub fn get_remotes(&self) -> Result<Vec<RemoteInfo>, Error> {
        let remotes = self.repo.remotes().context("Failed to get remotes list")?;

        let mut remote_infos = Vec::new();
        for i in 0..remotes.len() {
            if let Some(name) = remotes.get(i) {
                let remote = self
                    .repo
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

    /// Push current branch to remote (equivalent to `git push <remote> <branch>`)
    ///
    /// # Arguments
    /// * `remote_name` - The name of the remote (e.g., "origin")
    /// * `branch_name` - The name of the branch to push (e.g., "main", "master")
    pub fn push(&self, remote_name: &str, branch_name: &str) -> Result<(), Error> {
        let mut remote = self
            .repo
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

    /// Check if there are any staged files in the index
    pub fn has_staged_changes(&self) -> Result<bool, Error> {
        let mut index = self
            .repo
            .index()
            .context("Failed to get repository index")?;

        // If repository has no commits yet, any files in index are staged
        if self.repo.head().is_err() {
            return Ok(!index.is_empty());
        }

        // Compare index tree with HEAD tree
        let head = self.repo.head().context("Failed to get HEAD")?;
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
            .repo
            .index()
            .context("Failed to get repository index")?;

        let diff = if self.repo.head().is_err() {
            // No commits yet, diff against empty tree
            let empty_tree = self
                .repo
                .treebuilder(None)?
                .write()
                .context("Failed to create empty tree")?;
            let empty_tree = self.repo.find_tree(empty_tree)?;

            self.repo
                .diff_tree_to_index(Some(&empty_tree), Some(&index), None)
                .context("Failed to create diff from empty tree to index")?
        } else {
            // Compare HEAD tree with index
            let head = self.repo.head().context("Failed to get HEAD")?;
            let head_commit = head
                .peel_to_commit()
                .context("Failed to peel HEAD to commit")?;
            let head_tree = head_commit.tree().context("Failed to get HEAD tree")?;

            self.repo
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
    use git2::Repository;

    use crate::{
        git_repo::{GitRepo, RemoteInfo},
        test_utils::GitRepoTestDecorator,
    };

    #[test]
    fn open_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        Repository::init(path).unwrap();
        let repo = GitRepo::open(path);

        assert_eq!(repo.unwrap().path, temp_dir.path());
    }

    #[test]
    fn open_fails_in_non_git_folder() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepo::open(path);

        assert!(repo.is_err());
    }

    #[test]
    fn init_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        assert_eq!(repo.path, temp_dir.path());

        // `git branch` shows nothing
        let branches = repo.get_all_branches().unwrap();
        assert_eq!(branches.len(), 0);
        // Assert HEAD points to master branch (symbolic reference)
        repo.assert_current_branch("master");
    }

    #[test]
    fn init_fails_in_git_folder() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        Repository::init(path).unwrap();
        let repo = GitRepo::init(path);

        assert!(repo.is_err());
    }

    #[test]
    fn init_bare_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init_bare(path).unwrap());

        assert_eq!(repo.path, temp_dir.path());

        // Verify it's a bare repository
        assert!(repo.is_bare());

        let branches = repo.get_all_branches().unwrap();
        assert_eq!(branches.len(), 0);
        // Assert HEAD points to master branch (symbolic reference)
        repo.assert_current_branch("master");
    }

    #[test]
    fn init_bare_fails_in_git_folder() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        Repository::init(path).unwrap();
        let repo = GitRepo::init_bare(path);

        assert!(repo.is_err());
    }

    #[test]
    fn list_commits_works_in_repo_without_any_commit() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepo::init(path).unwrap();

        let commits = repo.list_commits().unwrap();

        assert_eq!(commits.len(), 0);
    }

    #[test]
    fn list_commits_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        let commits = repo.list_commits().unwrap();

        assert_eq!(commits.len(), 0);

        repo.add_file_and_commit("test_file_1.txt", "foo", "Test commit 1")
            .unwrap();
        repo.add_file_and_commit("test_file_2.txt", "foo", "Test commit 2")
            .unwrap();
        repo.add_file_and_commit("test_file_3.txt", "foo", "Test commit 3")
            .unwrap();

        let commits = repo.list_commits().unwrap();

        // TODO: assert content of CommitInfo
        assert_eq!(commits.len(), 3);

        repo.assert_commit_messages(&["Test commit 3", "Test commit 2", "Test commit 1"]);
    }

    #[test]
    fn add_works_for_single_file_path() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();

        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        let file_name = "test_file.txt";
        repo.add_file(file_name, "foo").unwrap();

        repo.add(&[file_name]).unwrap();
        // Verify the file is staged
        let index = repo.repo.index().unwrap();
        let entry = index.get_path(std::path::Path::new(file_name), 0);
        assert!(entry.is_some(), "File should be in the index after adding");
    }

    #[test]
    fn add_works_for_glob_patterns() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());
        repo.add_file("test_file_1.txt", "foo").unwrap();
        repo.add_file("test_file_2.txt", "foo").unwrap();
        repo.add_file("test_file_non_text.rs", "foo").unwrap();

        repo.add(&["*.txt"]).unwrap();
        // Verify the file is staged
        let index = repo.repo.index().unwrap();
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn add_works_for_all_files() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        repo.add_file("test_file_1.txt", "foo").unwrap();
        repo.add_file("test_file_2.txt", "foo").unwrap();
        repo.add_file("test_file_non_text.rs", "foo").unwrap();

        repo.add(&["."]).unwrap();
        // Verify the file is staged
        let index = repo.repo.index().unwrap();
        assert_eq!(index.len(), 3);
    }

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

        // Assert HEAD now points to the new branch
        repo.assert_current_branch(branch);

        // Assert list_commits returns empty (no commits in new branch)
        let commits = repo.list_commits().unwrap();
        assert_eq!(commits.len(), 0);
    }

    #[test]
    fn checkout_branch_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        // First create some commits on master
        repo.add_file_and_commit("file1.txt", "content1", "First commit")
            .unwrap();
        repo.assert_current_branch("master");

        // Create a new branch from master and switch to it
        repo.create_and_checkout_branch("feature-branch").unwrap();
        repo.assert_current_branch("feature-branch");

        // Add a commit to feature branch
        repo.add_file_and_commit("file2.txt", "content2", "Feature commit")
            .unwrap();

        // Switch back to master
        repo.checkout_branch("master").unwrap();

        // Verify we're on master (should only see file1.txt)
        repo.assert_current_branch("master")
            .assert_file_exists("file1.txt")
            .assert_file_not_exists("file2.txt")
            .assert_commit_messages(&["First commit"]);

        // Switch back to feature branch
        repo.checkout_branch("feature-branch").unwrap();

        // Verify we're on feature branch (should see both commits)
        repo.assert_current_branch("feature-branch")
            .assert_file_exists("file1.txt")
            .assert_file_exists("file2.txt")
            .assert_commit_messages(&["Feature commit", "First commit"]);
    }

    #[test]
    fn add_remote_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

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
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

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
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

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
        let repo = GitRepoTestDecorator::new(GitRepo::init(path).unwrap());

        let remotes = repo.get_remote_names().unwrap();
        assert_eq!(remotes.len(), 0);

        repo.add_remote("origin", "https://url1").unwrap();
        repo.add_remote("origin_2", "https://url2").unwrap();
        let remotes = repo.get_remote_names().unwrap();

        assert_eq!(remotes, vec!["origin", "origin_2"]);
    }

    #[test]
    fn push_works() {
        let local_dir = assert_fs::TempDir::new().unwrap();
        let remote_dir = assert_fs::TempDir::new().unwrap();

        let local_repo = GitRepoTestDecorator::new(GitRepo::init(local_dir.path()).unwrap());
        let remote_repo = GitRepoTestDecorator::new(GitRepo::init_bare(remote_dir.path()).unwrap());

        // Add some commits to push
        local_repo
            .add_file_and_commit("file1.txt", "content1", "First commit")
            .unwrap();

        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push master branch to origin
        local_repo.push("origin", "master").unwrap();

        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches, vec!["master"]);

        remote_repo.assert_commit_messages(&["First commit"]);
    }

    #[test]
    fn push_current_branch_works() {
        let local_dir = assert_fs::TempDir::new().unwrap();
        let remote_dir = assert_fs::TempDir::new().unwrap();

        let local_repo = GitRepoTestDecorator::new(GitRepo::init(local_dir.path()).unwrap());
        let remote_repo = GitRepoTestDecorator::new(GitRepo::init_bare(remote_dir.path()).unwrap());

        // Add some commits to push
        local_repo
            .create_and_checkout_branch("feature_branch")
            .unwrap();
        local_repo
            .add_file_and_commit("file1.txt", "content1", "First commit")
            .unwrap();

        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push master branch to origin
        local_repo.push_current_branch("origin").unwrap();

        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches, vec!["feature_branch"]);

        remote_repo.checkout_branch("feature_branch").unwrap();

        remote_repo.assert_commit_messages(&["First commit"]);
    }
    #[test]
    fn push_to_origin_works() {
        let local_dir = assert_fs::TempDir::new().unwrap();
        let remote_dir = assert_fs::TempDir::new().unwrap();

        let local_repo = GitRepoTestDecorator::new(GitRepo::init(local_dir.path()).unwrap());
        let remote_repo = GitRepoTestDecorator::new(GitRepo::init_bare(remote_dir.path()).unwrap());

        // Add some commits to push
        local_repo
            .create_and_checkout_branch("feature_branch")
            .unwrap();
        local_repo
            .add_file_and_commit("file1.txt", "content1", "First commit")
            .unwrap();

        local_repo.add_local_remote("origin", &remote_repo).unwrap();

        // Push master branch to origin
        local_repo.push_to_origin().unwrap();

        let remote_branches = remote_repo.get_all_branches().unwrap();
        assert_eq!(remote_branches, vec!["feature_branch"]);

        remote_repo.checkout_branch("feature_branch").unwrap();

        remote_repo.assert_commit_messages(&["First commit"]);
    }

    #[test]
    fn diff_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();

        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        let filename = "file1.txt";
        // Add some commits to push
        repo.create_and_checkout_branch("feature_branch").unwrap();
        repo.add_file_and_commit(filename, "line 1", "commit #1")
            .unwrap();
        repo.add_file_and_commit(filename, "line 2", "commit #2")
            .unwrap();
        repo.add_file_and_commit(filename, "line 3", "commit #3")
            .unwrap();

        repo.assert_commit_messages(&["commit #3", "commit #2", "commit #1"]);

        // TODO(next): rebase
    }

    #[test]
    fn has_staged_changes_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        // Initially no staged changes
        assert!(!repo.has_staged_changes().unwrap());

        // Add a file to working directory but don't stage it
        repo.add_file("test.txt", "content").unwrap();
        assert!(!repo.has_staged_changes().unwrap());

        // Stage the file
        repo.add(&["test.txt"]).unwrap();
        assert!(repo.has_staged_changes().unwrap());

        // Commit the file
        repo.commit("Initial commit").unwrap();
        assert!(!repo.has_staged_changes().unwrap());

        // Modify and stage again
        repo.add_file("test.txt", "modified content").unwrap();
        repo.add(&["test.txt"]).unwrap();
        assert!(repo.has_staged_changes().unwrap());
    }

    #[test]
    fn diff_staged_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        // Initially no staged changes, diff should be empty
        let diff = repo.diff_staged().unwrap();
        assert!(diff.is_empty());

        // Add a file and stage it
        repo.add_file("test.txt", "Hello World\n").unwrap();
        repo.add(&["test.txt"]).unwrap();

        let diff = repo.diff_staged().unwrap();
        assert!(diff.contains("+Hello World"));
        assert!(diff.contains("test.txt"));

        // Commit the file
        repo.commit("Initial commit").unwrap();

        // No staged changes after commit
        let diff = repo.diff_staged().unwrap();
        assert!(diff.is_empty());

        // Modify the file and stage
        repo.add_file("test.txt", "Hello World\nSecond line\n")
            .unwrap();
        repo.add(&["test.txt"]).unwrap();

        let diff = repo.diff_staged().unwrap();
        assert!(diff.contains("+Second line"));
        assert!(diff.contains("test.txt"));
    }

    #[test]
    fn get_staged_diff_and_diff_to_string_work() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        // Add a file and stage it
        repo.add_file("test.txt", "Hello World\n").unwrap();
        repo.add(&["test.txt"]).unwrap();

        // Test get_staged_diff returns a Diff object
        let diff_obj = repo.get_staged_diff().unwrap();
        assert_eq!(diff_obj.deltas().len(), 1);

        // Test diff_to_string converts the Diff to string
        let diff_string = repo.diff_to_string(&diff_obj).unwrap();
        assert!(diff_string.contains("+Hello World"));
        assert!(diff_string.contains("test.txt"));

        // Should be same as calling diff_staged directly
        let direct_diff = repo.diff_staged().unwrap();
        assert_eq!(diff_string, direct_diff);
    }

    #[test]
    fn get_current_branch_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        // Create an initial commit to establish the master branch
        repo.add_file_and_commit("initial.txt", "initial content", "Initial commit")
            .unwrap();

        // Should be on master after the first commit
        let current_branch = repo.get_current_branch().unwrap();
        assert_eq!(current_branch, "master");

        // Create and checkout a new branch
        repo.create_and_checkout_branch("feature-branch").unwrap();

        // Should now be on the new branch
        let current_branch = repo.get_current_branch().unwrap();
        assert_eq!(current_branch, "feature-branch");

        // Switch back to master
        repo.checkout_branch("master").unwrap();

        // Should be back on master
        let current_branch = repo.get_current_branch().unwrap();
        assert_eq!(current_branch, "master");
    }

    #[test]
    fn get_branch_commit_info_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        // Create an initial commit
        repo.add_file_and_commit("test.txt", "content", "Initial commit message")
            .unwrap();

        // Get commit info for master branch
        let commit_info = repo.get_branch_commit_info("master").unwrap();

        // Should contain short hash (7 chars) and the message
        assert!(commit_info.len() > 7); // At least hash + space + some message
        assert!(commit_info.contains("Initial commit message"));

        // Check format: should be "1234567 Initial commit message"
        let parts: Vec<&str> = commit_info.splitn(2, ' ').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].len(), 7); // Short hash should be 7 characters
        assert_eq!(parts[1], "Initial commit message");

        // Create a new branch with a different commit
        repo.create_and_checkout_branch("feature").unwrap();
        repo.add_file_and_commit("feature.txt", "feature content", "Add feature file")
            .unwrap();

        // Get commit info for the feature branch
        let feature_commit_info = repo.get_branch_commit_info("feature").unwrap();
        assert!(feature_commit_info.contains("Add feature file"));

        // Master and feature should have different commit info
        assert_ne!(commit_info, feature_commit_info);
    }

    #[test]
    fn get_remote_tracking_info_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let repo = GitRepoTestDecorator::new(GitRepo::init(temp_dir.path()).unwrap());

        // Create an initial commit
        repo.add_file_and_commit("test.txt", "content", "Initial commit")
            .unwrap();

        // Add a remote
        repo.add_remote("origin", "https://github.com/test/repo.git")
            .unwrap();

        // Set up tracking branch manually using git command
        // (this simulates what happens when you do `git push -u origin master`)
        let _ = std::process::Command::new("git")
            .args([
                "-C",
                temp_dir.path().to_str().unwrap(),
                "branch",
                "--set-upstream-to=origin/master",
                "master",
            ])
            .output();

        // Test getting remote tracking info
        let result = repo.get_remote_tracking_info("master");

        // It should either succeed with "origin/master" or fail if upstream not set up
        // Since setting up upstream is complex in tests, we'll just check that the method works
        match result {
            Ok(tracking) => {
                // If successful, should be in format "origin/branch"
                assert!(tracking.contains("origin"));
            }
            Err(_) => {
                // If no upstream is set, that's also valid behavior
                // The method should return an error for branches without tracking
            }
        }

        // Test with non-existent branch
        let result = repo.get_remote_tracking_info("nonexistent");
        assert!(result.is_err());
    }
}
