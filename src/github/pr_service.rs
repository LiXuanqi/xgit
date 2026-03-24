use crate::{github::client::GitHubClient, tui::branch_display::PullRequestState};
use anyhow::{Context, Error};
use serde::Deserialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PullRequestRecord {
    pub number: u64,
    pub state: PullRequestState,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub head_sha: String,
    pub merged: bool,
}

impl PullRequestRecord {
    pub fn is_closed_or_merged(&self) -> bool {
        matches!(self.state, PullRequestState::Closed) || self.merged
    }
}

enum Backend {
    GhCli,
    Api(GitHubClient),
}

pub struct GitHubPrService {
    backend: Backend,
    repo_slug: String,
    repo_path: PathBuf,
}

impl GitHubPrService {
    pub fn new(repo_path: &Path, owner: String, repo: String) -> Result<Self, Error> {
        let backend = match env::var("XGIT_GITHUB_BACKEND").ok().as_deref() {
            Some("api") => Backend::Api(GitHubClient::new(owner.clone(), repo.clone())?),
            Some("gh") => Backend::GhCli,
            _ => Backend::GhCli,
        };

        Ok(Self {
            backend,
            repo_slug: format!("{owner}/{repo}"),
            repo_path: repo_path.to_path_buf(),
        })
    }

    pub fn ensure_ready(&self) -> Result<(), Error> {
        match self.backend {
            Backend::GhCli => {
                let version = Command::new("gh")
                    .arg("--version")
                    .current_dir(&self.repo_path)
                    .output()
                    .context("Failed to execute gh --version. Please install GitHub CLI (`gh`)")?;
                if !version.status.success() {
                    return Err(anyhow::anyhow!(
                        "GitHub CLI (`gh`) is required for xg GitHub operations"
                    ));
                }

                Ok(())
            }
            Backend::Api(_) => Ok(()),
        }
    }

    pub async fn get_default_branch(&self) -> Result<String, Error> {
        match &self.backend {
            Backend::GhCli => {
                let output = gh_output(
                    &self.repo_path,
                    &[
                        "api",
                        &format!("repos/{}", self.repo_slug),
                        "--jq",
                        ".default_branch",
                    ],
                )?;
                Ok(output.trim().to_string())
            }
            Backend::Api(client) => client.get_default_branch().await,
        }
    }

    pub async fn get_pr(&self, pr_number: u64) -> Result<PullRequestRecord, Error> {
        match &self.backend {
            Backend::GhCli => gh_pr_view(&self.repo_path, &self.repo_slug, pr_number),
            Backend::Api(client) => {
                let pr = client.get_pr_by_number(pr_number).await?;
                Ok(PullRequestRecord {
                    number: pr.number,
                    state: pr.state,
                    url: pr.url,
                    base_ref: pr.base_ref,
                    head_ref: pr.head_ref,
                    head_sha: pr.head_sha,
                    merged: pr.merged,
                })
            }
        }
    }

    pub async fn create_pr(
        &self,
        title: &str,
        body: Option<&str>,
        head: &str,
        base: &str,
        draft: bool,
    ) -> Result<PullRequestRecord, Error> {
        match &self.backend {
            Backend::GhCli => gh_pr_create(
                &self.repo_path,
                &self.repo_slug,
                title,
                body,
                head,
                base,
                draft,
            ),
            Backend::Api(client) => {
                let pr = client.create_pr(title, body, head, base, draft).await?;
                Ok(PullRequestRecord {
                    number: pr.number,
                    state: pr.state,
                    url: pr.url,
                    base_ref: pr.base_ref,
                    head_ref: pr.head_ref,
                    head_sha: pr.head_sha,
                    merged: pr.merged,
                })
            }
        }
    }

    pub async fn update_pr(
        &self,
        pr_number: u64,
        base: Option<&str>,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<PullRequestRecord, Error> {
        match &self.backend {
            Backend::GhCli => {
                gh_pr_edit(
                    &self.repo_path,
                    &self.repo_slug,
                    pr_number,
                    base,
                    title,
                    body,
                )?;
                gh_pr_view(&self.repo_path, &self.repo_slug, pr_number)
            }
            Backend::Api(client) => {
                let pr = client.update_pr(pr_number, base, title, body).await?;
                Ok(PullRequestRecord {
                    number: pr.number,
                    state: pr.state,
                    url: pr.url,
                    base_ref: pr.base_ref,
                    head_ref: pr.head_ref,
                    head_sha: pr.head_sha,
                    merged: pr.merged,
                })
            }
        }
    }

    pub async fn find_pr_by_head(&self, head_branch: &str) -> Result<PullRequestRecord, Error> {
        match &self.backend {
            Backend::GhCli => gh_pr_find_by_head(&self.repo_path, &self.repo_slug, head_branch),
            Backend::Api(client) => {
                let found = client
                    .find_pr_by_head_branch(head_branch)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!("No PR found for head branch '{head_branch}'")
                    })?;
                self.get_pr(found.number).await
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct GhPrViewResponse {
    number: u64,
    state: String,
    url: String,
    #[serde(rename = "baseRefName")]
    base_ref_name: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

fn gh_pr_view(
    repo_path: &Path,
    repo_slug: &str,
    pr_number: u64,
) -> Result<PullRequestRecord, Error> {
    let output = gh_output(
        repo_path,
        &[
            "pr",
            "view",
            &pr_number.to_string(),
            "--repo",
            repo_slug,
            "--json",
            "number,state,url,baseRefName,headRefName,headRefOid,mergedAt",
        ],
    )?;
    let parsed: GhPrViewResponse =
        serde_json::from_str(&output).context("Failed to parse `gh pr view` JSON output")?;
    Ok(PullRequestRecord {
        number: parsed.number,
        state: gh_state_to_pull_request_state(&parsed.state),
        url: parsed.url,
        base_ref: parsed.base_ref_name,
        head_ref: parsed.head_ref_name,
        head_sha: parsed.head_ref_oid,
        merged: parsed.merged_at.is_some(),
    })
}

fn gh_pr_create(
    repo_path: &Path,
    repo_slug: &str,
    title: &str,
    body: Option<&str>,
    head: &str,
    base: &str,
    draft: bool,
) -> Result<PullRequestRecord, Error> {
    let mut args = vec![
        "pr".to_string(),
        "create".to_string(),
        "--repo".to_string(),
        repo_slug.to_string(),
        "--title".to_string(),
        title.to_string(),
        "--head".to_string(),
        head.to_string(),
        "--base".to_string(),
        base.to_string(),
    ];

    if let Some(body) = body {
        args.push("--body".to_string());
        args.push(body.to_string());
    } else {
        args.push("--body".to_string());
        args.push("".to_string());
    }

    if draft {
        args.push("--draft".to_string());
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    gh_output(repo_path, &arg_refs).context("`gh pr create` failed")?;
    gh_pr_find_by_head(repo_path, repo_slug, head)
        .context("PR was created but could not be resolved by head branch")
}

fn gh_pr_edit(
    repo_path: &Path,
    repo_slug: &str,
    pr_number: u64,
    base: Option<&str>,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<(), Error> {
    let mut args = vec![
        "pr".to_string(),
        "edit".to_string(),
        pr_number.to_string(),
        "--repo".to_string(),
        repo_slug.to_string(),
    ];

    if let Some(base) = base {
        args.push("--base".to_string());
        args.push(base.to_string());
    }
    if let Some(title) = title {
        args.push("--title".to_string());
        args.push(title.to_string());
    }
    if let Some(body) = body {
        args.push("--body".to_string());
        args.push(body.to_string());
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    gh_output(repo_path, &arg_refs).context("`gh pr edit` failed")?;
    Ok(())
}

fn gh_pr_find_by_head(
    repo_path: &Path,
    repo_slug: &str,
    head_branch: &str,
) -> Result<PullRequestRecord, Error> {
    let output = gh_output(
        repo_path,
        &[
            "pr",
            "list",
            "--repo",
            repo_slug,
            "--head",
            head_branch,
            "--state",
            "all",
            "--limit",
            "1",
            "--json",
            "number,state,url,baseRefName,headRefName,headRefOid,mergedAt",
        ],
    )?;

    let parsed: Vec<GhPrViewResponse> =
        serde_json::from_str(&output).context("Failed to parse `gh pr list` JSON output")?;
    let first = parsed
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No PR found for head branch '{}'", head_branch))?;

    Ok(PullRequestRecord {
        number: first.number,
        state: gh_state_to_pull_request_state(&first.state),
        url: first.url,
        base_ref: first.base_ref_name,
        head_ref: first.head_ref_name,
        head_sha: first.head_ref_oid,
        merged: first.merged_at.is_some(),
    })
}

fn gh_state_to_pull_request_state(state: &str) -> PullRequestState {
    if state.eq_ignore_ascii_case("closed") || state.eq_ignore_ascii_case("merged") {
        PullRequestState::Closed
    } else {
        PullRequestState::Open
    }
}

fn gh_output(repo_path: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(repo_path)
        .output()
        .context("Failed to execute gh command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "gh {:?} failed (code {:?}): {}",
            args,
            output.status.code(),
            stderr.trim()
        ));
    }

    String::from_utf8(output.stdout).context("Invalid UTF-8 gh output")
}
