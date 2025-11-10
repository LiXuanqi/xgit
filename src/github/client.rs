use crate::tui::branch_display::{PullRequestInfo, PullRequestState};
use anyhow::{Context, Error};
use octocrab::Octocrab;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct PullRequestDetails {
    pub number: u64,
    pub title: String,
    pub state: PullRequestState,
    pub url: String,
    pub draft: bool,
    pub base_ref: String,
    pub head_ref: String,
    pub merged: bool,
}

pub struct GitHubClient {
    octocrab: Octocrab,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(owner: String, repo: String) -> Result<Self, Error> {
        let octocrab = Octocrab::builder()
            .build()
            .context("Failed to create GitHub client")?;

        Ok(Self {
            octocrab,
            owner,
            repo,
        })
    }

    pub async fn find_pr_by_head_branch(
        &self,
        branch: &str,
    ) -> Result<Option<PullRequestInfo>, Error> {
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
            let pr_info = to_pull_request_info(pr);
            Ok(Some(pr_info))
        } else {
            Ok(None)
        }
    }

    pub async fn find_pr_by_head_branch_with_owner(
        &self,
        owner: &str,
        branch: &str,
    ) -> Result<Option<PullRequestInfo>, Error> {
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
            let pr_info = to_pull_request_info(pr);
            Ok(Some(pr_info))
        } else {
            Ok(None)
        }
    }

    pub async fn get_pr_by_number(&self, pr_number: u64) -> Result<PullRequestDetails, Error> {
        let pr = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .get(pr_number)
            .await
            .context("Failed to fetch pull request by number")?;
        Ok(to_pull_request_details(&pr))
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
    ) -> Result<PullRequestDetails, Error> {
        let pulls = self.octocrab.pulls(&self.owner, &self.repo);
        let mut builder = pulls.create(title, head, base).draft(draft);
        if let Some(body) = body {
            builder = builder.body(body.to_string());
        }

        let pr = builder
            .send()
            .await
            .context("Failed to create pull request")?;

        Ok(to_pull_request_details(&pr))
    }

    pub async fn update_pr(
        &self,
        pr_number: u64,
        base: Option<&str>,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<PullRequestDetails, Error> {
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
        Ok(to_pull_request_details(&pr))
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

fn to_pull_request_info(pr: &octocrab::models::pulls::PullRequest) -> PullRequestInfo {
    PullRequestInfo {
        number: pr.number,
        title: pr.title.clone().unwrap_or_default(),
        state: to_pull_request_state(pr.state.clone()),
        url: pr
            .html_url
            .as_ref()
            .map(|u| u.to_string())
            .unwrap_or_default(),
        draft: pr.draft.unwrap_or(false),
    }
}

fn to_pull_request_details(pr: &octocrab::models::pulls::PullRequest) -> PullRequestDetails {
    PullRequestDetails {
        number: pr.number,
        title: pr.title.clone().unwrap_or_default(),
        state: to_pull_request_state(pr.state.clone()),
        url: pr
            .html_url
            .as_ref()
            .map(|u| u.to_string())
            .unwrap_or_default(),
        draft: pr.draft.unwrap_or(false),
        base_ref: pr.base.ref_field.clone(),
        head_ref: pr.head.ref_field.clone(),
        merged: pr.merged_at.is_some(),
    }
}

fn to_pull_request_state(state: Option<octocrab::models::IssueState>) -> PullRequestState {
    match state {
        Some(octocrab::models::IssueState::Open) => PullRequestState::Open,
        Some(octocrab::models::IssueState::Closed) => PullRequestState::Closed,
        Some(_) => PullRequestState::Open,
        None => PullRequestState::Open,
    }
}
