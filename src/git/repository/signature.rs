use anyhow::{Context, Error};
use git2::Signature;

use super::core::GitRepo;

impl GitRepo {
    pub(crate) fn create_signature(&self) -> Result<Signature<'_>, Error> {
        let config = self
            .repo()
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
}
