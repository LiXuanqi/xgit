use crate::{
    git::GitRepo,
    github::{
        pr_service::GitHubPrService,
        types::{PullRequestRecord, ResolvedPullRequest},
    },
};
use anyhow::{Context, Error};

pub struct GitHubPrMatcher {
    service: GitHubPrService,
    github_remote: String,
}

impl GitHubPrMatcher {
    pub fn new(repo: &GitRepo) -> Result<Self, Error> {
        let (owner, repo_name) = get_github_repo_info(repo)?;
        let github_remote = get_github_remote(repo)?;
        let service = GitHubPrService::new(repo.path(), owner, repo_name)?;

        Ok(Self {
            service,
            github_remote,
        })
    }

    pub fn service(&self) -> &GitHubPrService {
        &self.service
    }

    pub fn remote_name(&self) -> &str {
        &self.github_remote
    }

    pub async fn find_pr_for_branch(
        &self,
        repo: &GitRepo,
        branch: &str,
    ) -> Option<ResolvedPullRequest> {
        let remote_tracking = repo.get_remote_tracking_info(branch).ok();
        let remote_branch = remote_tracking.as_deref().map(extract_branch_name);

        if let Ok(Some(cached)) = self.service.get_cached_by_branch(branch) {
            return self
                .refresh_or_fallback(cached, branch, remote_branch.as_deref(), true)
                .await
                .ok()
                .flatten();
        }

        if let Some(ref remote_branch_name) = remote_branch {
            if let Ok(Some(cached)) = self.service.get_cached_by_remote_head(remote_branch_name) {
                return self
                    .refresh_or_fallback(cached, branch, Some(remote_branch_name), true)
                    .await
                    .ok()
                    .flatten();
            }
        }

        self.lookup_live(repo, branch, remote_branch.as_deref(), true)
            .await
            .ok()
            .flatten()
    }

    pub async fn refresh_pr_for_branch(
        &self,
        repo: &GitRepo,
        branch: &str,
    ) -> Result<Option<ResolvedPullRequest>, Error> {
        let remote_tracking = repo.get_remote_tracking_info(branch).ok();
        let remote_branch = remote_tracking.as_deref().map(extract_branch_name);

        if let Some(cached) = self.service.get_cached_by_branch(branch)?.or_else(|| {
            remote_branch.as_deref().and_then(|remote_branch_name| {
                self.service
                    .get_cached_by_remote_head(remote_branch_name)
                    .ok()
                    .flatten()
            })
        }) {
            return self
                .refresh_or_fallback(cached, branch, remote_branch.as_deref(), false)
                .await;
        }

        self.lookup_live(repo, branch, remote_branch.as_deref(), false)
            .await
    }

    async fn refresh_or_fallback(
        &self,
        cached: PullRequestRecord,
        branch: &str,
        remote_branch: Option<&str>,
        allow_stale_on_error: bool,
    ) -> Result<Option<ResolvedPullRequest>, Error> {
        if cached.is_fresh(self.service.cache_ttl_secs()) {
            let cached = self.attach_associations(cached, branch, remote_branch)?;
            return Ok(Some(ResolvedPullRequest {
                record: cached,
                is_stale: false,
            }));
        }

        match self.service.get_pr(cached.pr_number).await {
            Ok(refreshed) => {
                let refreshed = self.attach_associations(refreshed, branch, remote_branch)?;
                Ok(Some(ResolvedPullRequest {
                    record: refreshed,
                    is_stale: false,
                }))
            }
            Err(err) if allow_stale_on_error => {
                let cached = self.attach_associations(cached, branch, remote_branch)?;
                let _ = err;
                Ok(Some(ResolvedPullRequest {
                    record: cached,
                    is_stale: true,
                }))
            }
            Err(err) => Err(err),
        }
    }

    async fn lookup_live(
        &self,
        repo: &GitRepo,
        branch: &str,
        remote_branch: Option<&str>,
        allow_stale_on_error: bool,
    ) -> Result<Option<ResolvedPullRequest>, Error> {
        if let Some(found) = self.service.find_pr_by_head(branch).await? {
            let found = self.attach_associations(found, branch, remote_branch)?;
            return Ok(Some(ResolvedPullRequest {
                record: found,
                is_stale: false,
            }));
        }

        if let Some(remote_branch_name) = remote_branch {
            if let Some(found) = self.service.find_pr_by_head(remote_branch_name).await? {
                let found = self.attach_associations(found, branch, Some(remote_branch_name))?;
                return Ok(Some(ResolvedPullRequest {
                    record: found,
                    is_stale: false,
                }));
            }
        }

        if let Ok(fork_owner) = get_fork_owner_from_remote(repo, &self.github_remote) {
            if let Some(found) = self
                .service
                .find_pr_by_head_with_owner(&fork_owner, branch)
                .await?
            {
                let found = self.attach_associations(found, branch, remote_branch)?;
                return Ok(Some(ResolvedPullRequest {
                    record: found,
                    is_stale: false,
                }));
            }
        }

        if allow_stale_on_error {
            return Ok(None);
        }

        Ok(None)
    }

    fn attach_associations(
        &self,
        record: PullRequestRecord,
        branch: &str,
        remote_branch: Option<&str>,
    ) -> Result<PullRequestRecord, Error> {
        let _ = self.service.attach_branch(record.pr_number, branch)?;
        if let Some(remote_branch) = remote_branch {
            let _ = self
                .service
                .attach_remote_head(record.pr_number, remote_branch)?;
        }

        Ok(self
            .service
            .get_cached_pr(record.pr_number)?
            .unwrap_or(record))
    }
}

fn get_github_repo_info(repo: &GitRepo) -> Result<(String, String), Error> {
    let remote_url = repo
        .get_remote_url("origin")
        .or_else(|_| repo.get_remote_url("upstream"))
        .context("Failed to get remote URL")?;

    parse_github_url(&remote_url)
}

fn get_github_remote(repo: &GitRepo) -> Result<String, Error> {
    for remote_name in ["origin", "upstream"] {
        if let Ok(url) = repo.get_remote_url(remote_name) {
            if url.contains("github.com") {
                return Ok(remote_name.to_string());
            }
        }
    }

    let remotes = repo.get_remotes().context("Failed to get remotes")?;
    for remote in remotes {
        if let Ok(url) = repo.get_remote_url(&remote.name) {
            if url.contains("github.com") {
                return Ok(remote.name);
            }
        }
    }

    Err(anyhow::anyhow!("No GitHub remote found"))
}

fn parse_github_url(url: &str) -> Result<(String, String), Error> {
    if let Some(ssh_part) = url.strip_prefix("git@github.com:") {
        let repo_part = ssh_part.strip_suffix(".git").unwrap_or(ssh_part);
        let parts: Vec<&str> = repo_part.split('/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    if let Some(https_part) = url.strip_prefix("https://github.com/") {
        let repo_part = https_part.strip_suffix(".git").unwrap_or(https_part);
        let parts: Vec<&str> = repo_part.split('/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    Err(anyhow::anyhow!("Invalid GitHub URL format: {}", url))
}

fn extract_branch_name(remote_tracking: &str) -> String {
    if let Some(slash_pos) = remote_tracking.find('/') {
        remote_tracking[slash_pos + 1..].to_string()
    } else {
        remote_tracking.to_string()
    }
}

fn get_fork_owner_from_remote(repo: &GitRepo, remote_name: &str) -> Result<String, Error> {
    let remote_url = repo.get_remote_url(remote_name)?;
    let (owner, _) = parse_github_url(&remote_url)?;
    Ok(owner)
}
