use crate::tui::branch_display::{PullRequestInfo, PullRequestState};
use anyhow::{Context, Error};
use octocrab::Octocrab;

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
            let pr_info = PullRequestInfo {
                number: pr.number,
                title: pr.title.clone().unwrap_or_default(),
                state: match pr.state {
                    Some(octocrab::models::IssueState::Open) => PullRequestState::Open,
                    Some(octocrab::models::IssueState::Closed) => PullRequestState::Closed,
                    Some(_) => PullRequestState::Open, // Handle any other states as Open
                    None => PullRequestState::Open,
                },
                url: pr
                    .html_url
                    .as_ref()
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
                draft: pr.draft.unwrap_or(false),
            };
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
            let pr_info = PullRequestInfo {
                number: pr.number,
                title: pr.title.clone().unwrap_or_default(),
                state: match pr.state {
                    Some(octocrab::models::IssueState::Open) => PullRequestState::Open,
                    Some(octocrab::models::IssueState::Closed) => PullRequestState::Closed,
                    Some(_) => PullRequestState::Open, // Handle any other states as Open
                    None => PullRequestState::Open,
                },
                url: pr
                    .html_url
                    .as_ref()
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
                draft: pr.draft.unwrap_or(false),
            };
            Ok(Some(pr_info))
        } else {
            Ok(None)
        }
    }
}
