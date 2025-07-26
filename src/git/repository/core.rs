use std::path::{Path, PathBuf};

use anyhow::{Context, Error};
use git2::Repository;

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

pub struct GitRepo {
    path: PathBuf,
    repo: Repository,
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

    /// Get the path to the repository
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if this is a bare repository
    pub fn is_bare(&self) -> bool {
        self.repo.is_bare()
    }

    /// Get access to the internal git2 Repository
    pub(crate) fn repo(&self) -> &Repository {
        &self.repo
    }
}

#[cfg(test)]
mod tests {
    use git2::Repository;

    use crate::{git::GitRepo, test_utils::RepoAssertions};

    #[test]
    fn open_works() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let path = temp_dir.path();
        Repository::init(path).unwrap();
        let repo = GitRepo::open(path);

        assert_eq!(repo.unwrap().path(), temp_dir.path());
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
        let repo = GitRepo::init(path).unwrap();

        assert_eq!(repo.path(), temp_dir.path());

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
        let repo = GitRepo::init_bare(path).unwrap();

        assert_eq!(repo.path(), temp_dir.path());

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
}
