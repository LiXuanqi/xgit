use crate::github::types::{PullRequestRecord, PullRequestSnapshot, PullRequestStatus};
use anyhow::{Context, Error};
use octocrab::Octocrab;
use serde_json::json;
use std::env;

pub struct GitHubClient {
    octocrab: Octocrab,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(owner: String, repo: String) -> Result<Self, Error> {
        let octocrab = build_octocrab_from_env().context("Failed to create GitHub client")?;

        Ok(Self {
            octocrab,
            owner,
            repo,
        })
    }

    pub async fn find_pr_by_head_branch(
        &self,
        branch: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let pulls = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .list()
            .state(octocrab::params::State::All)
            .head(format!("{}:{}", &self.owner, branch))
            .send()
            .await
            .context("Failed to fetch pull requests")?;

        if let Some(pr) = pulls.items.first() {
            let pr_info = to_pull_request_record(&self.owner, &self.repo, pr);
            Ok(Some(pr_info))
        } else {
            Ok(None)
        }
    }

    pub async fn find_pr_by_head_branch_with_owner(
        &self,
        owner: &str,
        branch: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let pulls = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .list()
            .state(octocrab::params::State::All)
            .head(format!("{owner}:{branch}"))
            .send()
            .await
            .context("Failed to fetch pull requests")?;

        if let Some(pr) = pulls.items.first() {
            let pr_info = to_pull_request_record(&self.owner, &self.repo, pr);
            Ok(Some(pr_info))
        } else {
            Ok(None)
        }
    }

    pub async fn get_pr_by_number(&self, pr_number: u64) -> Result<PullRequestRecord, Error> {
        let pr = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .get(pr_number)
            .await
            .context("Failed to fetch pull request by number")?;
        Ok(to_pull_request_record(&self.owner, &self.repo, &pr))
    }

    pub async fn get_default_branch(&self) -> Result<String, Error> {
        let repo = self
            .octocrab
            .repos(&self.owner, &self.repo)
            .get()
            .await
            .context("Failed to fetch repository metadata")?;

        repo.default_branch
            .ok_or_else(|| anyhow::anyhow!("Repository default branch is not available"))
    }

    pub async fn create_pr(
        &self,
        title: &str,
        body: Option<&str>,
        head: &str,
        base: &str,
        draft: bool,
    ) -> Result<PullRequestRecord, Error> {
        let pulls = self.octocrab.pulls(&self.owner, &self.repo);
        let mut builder = pulls.create(title, head, base).draft(draft);
        if let Some(body) = body {
            builder = builder.body(body.to_string());
        }

        let pr = builder
            .send()
            .await
            .context("Failed to create pull request")?;

        Ok(to_pull_request_record(&self.owner, &self.repo, &pr))
    }

    pub async fn update_pr(
        &self,
        pr_number: u64,
        base: Option<&str>,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<PullRequestRecord, Error> {
        let pulls = self.octocrab.pulls(&self.owner, &self.repo);
        let mut builder = pulls.update(pr_number);
        if let Some(base) = base {
            builder = builder.base(base);
        }
        if let Some(title) = title {
            builder = builder.title(title);
        }
        if let Some(body) = body {
            builder = builder.body(body.to_string());
        }

        let pr = builder
            .send()
            .await
            .context("Failed to update pull request")?;
        Ok(to_pull_request_record(&self.owner, &self.repo, &pr))
    }

    pub async fn rename_branch(&self, from: &str, to: &str) -> Result<(), Error> {
        let route = format!(
            "/repos/{owner}/{repo}/branches/{from}/rename",
            owner = self.owner,
            repo = self.repo,
            from = from
        );

        self.octocrab
            .post::<_, serde_json::Value>(route, Some(&json!({ "new_name": to })))
            .await
            .context("Failed to rename branch on GitHub")?;

        Ok(())
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }
}

fn build_octocrab_from_env() -> Result<Octocrab, Error> {
    let token = env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| env::var("GH_TOKEN").ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let builder = Octocrab::builder();
    let octocrab = match token {
        Some(token) => builder.personal_token(token).build(),
        None => builder.build(),
    }?;

    Ok(octocrab)
}

fn to_pull_request_record(
    owner: &str,
    repo: &str,
    pr: &octocrab::models::pulls::PullRequest,
) -> PullRequestRecord {
    PullRequestRecord::from_snapshot(PullRequestSnapshot {
        repo_slug: format!("{owner}/{repo}"),
        pr_number: pr.number,
        title: pr.title.clone().unwrap_or_default(),
        url: pr
            .html_url
            .as_ref()
            .map(|u| u.to_string())
            .unwrap_or_default(),
        base_ref: pr.base.ref_field.clone(),
        head_ref: pr.head.ref_field.clone(),
        head_sha: pr.head.sha.clone(),
        draft: pr.draft.unwrap_or(false),
        status: to_pull_request_status(pr),
    })
}

fn to_pull_request_status(pr: &octocrab::models::pulls::PullRequest) -> PullRequestStatus {
    if pr.merged_at.is_some() {
        return PullRequestStatus::Merged;
    }

    match pr.state.clone() {
        Some(octocrab::models::IssueState::Closed) => PullRequestStatus::Closed,
        Some(octocrab::models::IssueState::Open) => PullRequestStatus::Open,
        Some(_) | None => PullRequestStatus::Open,
    }
}
