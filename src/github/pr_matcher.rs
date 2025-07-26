use crate::{git::GitRepo, github::client::GitHubClient, tui::branch_display::PullRequestInfo};
use anyhow::{Context, Error};

pub struct GitHubPrMatcher {
    client: GitHubClient,
    github_remote: String,
}

impl GitHubPrMatcher {
    pub fn new(repo: &GitRepo) -> Result<Self, Error> {
        let (owner, repo_name) = get_github_repo_info(repo)?;
        let github_remote = get_github_remote(repo)?;
        let client = GitHubClient::new(owner, repo_name)?;

        Ok(Self {
            client,
            github_remote,
        })
    }

    pub async fn find_pr_for_branch(
        &self,
        repo: &GitRepo,
        branch: &str,
    ) -> Option<PullRequestInfo> {
        // Strategy 1: Direct head branch match
        if let Ok(Some(pr)) = self.client.find_pr_by_head_branch(branch).await {
            return Some(pr);
        }

        // Strategy 2: Use remote tracking branch name
        if let Ok(remote_tracking) = repo.get_remote_tracking_info(branch) {
            let remote_branch = extract_branch_name(&remote_tracking);
            if let Ok(Some(pr)) = self.client.find_pr_by_head_branch(&remote_branch).await {
                return Some(pr);
            }
        }

        // Strategy 3: Try with different owner (for forks)
        if let Ok(fork_owner) = get_fork_owner_from_remote(repo, &self.github_remote)
            && let Ok(Some(pr)) = self
                .client
                .find_pr_by_head_branch_with_owner(&fork_owner, branch)
                .await
        {
            return Some(pr);
        }

        None
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
    // Try common remote names in order of preference
    for remote_name in ["origin", "upstream"] {
        if let Ok(url) = repo.get_remote_url(remote_name)
            && url.contains("github.com")
        {
            return Ok(remote_name.to_string());
        }
    }

    // Fallback to first GitHub remote found
    let remotes = repo.get_remotes().context("Failed to get remotes")?;
    for remote in remotes {
        if let Ok(url) = repo.get_remote_url(&remote.name)
            && url.contains("github.com")
        {
            return Ok(remote.name);
        }
    }

    Err(anyhow::anyhow!("No GitHub remote found"))
}

fn parse_github_url(url: &str) -> Result<(String, String), Error> {
    // Handle SSH format: git@github.com:owner/repo.git
    if let Some(ssh_part) = url.strip_prefix("git@github.com:") {
        let repo_part = ssh_part.strip_suffix(".git").unwrap_or(ssh_part);
        let parts: Vec<&str> = repo_part.split('/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // Handle HTTPS format: https://github.com/owner/repo.git
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
    // Extract "branch-name" from "origin/branch-name" or "upstream/branch-name"
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
