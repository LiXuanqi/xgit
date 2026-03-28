use crate::{
    git::GitRepo,
    github::{
        client::GitHubClient,
        pr_index::{JsonPrIndexStore, PrIndexStore},
        types::{PullRequestRecord, PullRequestSnapshot, PullRequestStatus},
    },
};
use anyhow::{Context, Error};
use serde::Deserialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_CACHE_TTL_SECS: u64 = 300;

enum Backend {
    GhCli,
    Api(GitHubClient),
}

pub struct GitHubPrService {
    backend: Backend,
    repo_slug: String,
    repo_path: PathBuf,
    store: Box<dyn PrIndexStore>,
    cache_ttl_secs: u64,
}

impl GitHubPrService {
    pub fn new(repo_path: &Path, owner: String, repo: String) -> Result<Self, Error> {
        let backend = match env::var("XGIT_GITHUB_BACKEND").ok().as_deref() {
            Some("api") => Backend::Api(GitHubClient::new(owner.clone(), repo.clone())?),
            Some("gh") => Backend::GhCli,
            _ => Backend::GhCli,
        };
        let discovered_repo = git2::Repository::discover(repo_path)
            .context("Failed to discover repository for PR index")?;
        let index_path = discovered_repo.path().join("xgit").join("pr-index.json");

        Ok(Self {
            backend,
            repo_slug: format!("{owner}/{repo}"),
            repo_path: repo_path.to_path_buf(),
            store: Box::new(JsonPrIndexStore::new(index_path)),
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
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

    pub fn repo_slug(&self) -> &str {
        &self.repo_slug
    }

    pub fn cache_ttl_secs(&self) -> u64 {
        self.cache_ttl_secs
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

    pub async fn resolve_trunk_base_branch(&self, repo: &GitRepo) -> Result<String, Error> {
        if let Ok(default_branch) = self.get_default_branch().await {
            return Ok(default_branch);
        }

        let branches = repo.get_all_branches()?;
        if branches.iter().any(|branch| branch == "main") {
            return Ok("main".to_string());
        }
        if branches.iter().any(|branch| branch == "master") {
            return Ok("master".to_string());
        }

        Err(anyhow::anyhow!(
            "Unable to determine trunk branch from GitHub default branch or local main/master"
        ))
    }

    pub fn get_cached_pr(&self, pr_number: u64) -> Result<Option<PullRequestRecord>, Error> {
        self.store.get_by_pr(&self.repo_slug, pr_number)
    }

    pub fn get_cached_by_branch(
        &self,
        branch_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.store.get_by_branch(&self.repo_slug, branch_name)
    }

    pub fn get_cached_by_remote_head(
        &self,
        remote_head_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.store
            .get_by_remote_head(&self.repo_slug, remote_head_name)
    }

    pub fn get_cached_by_commit(
        &self,
        commit_sha: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.store.get_by_commit(&self.repo_slug, commit_sha)
    }

    pub fn attach_branch(
        &self,
        pr_number: u64,
        branch_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.store
            .attach_branch(&self.repo_slug, pr_number, branch_name)
    }

    pub fn attach_remote_head(
        &self,
        pr_number: u64,
        remote_head_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.store
            .attach_remote_head(&self.repo_slug, pr_number, remote_head_name)
    }

    pub fn attach_commit(
        &self,
        pr_number: u64,
        commit_sha: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.store
            .attach_commit(&self.repo_slug, pr_number, commit_sha)
    }

    pub async fn hydrate_pr_from_commit(
        &self,
        pr_number: u64,
        commit_sha: &str,
        branch_name: Option<&str>,
    ) -> Result<PullRequestRecord, Error> {
        let record = match self.get_cached_pr(pr_number)? {
            Some(record) => record,
            None => self.get_pr(pr_number).await?,
        };

        self.attach_commit(pr_number, commit_sha)?;
        if let Some(branch_name) = branch_name {
            self.attach_branch(pr_number, branch_name)?;
        }

        Ok(self.get_cached_pr(pr_number)?.unwrap_or(record))
    }

    pub async fn get_pr(&self, pr_number: u64) -> Result<PullRequestRecord, Error> {
        let live = match &self.backend {
            Backend::GhCli => gh_pr_view(&self.repo_path, &self.repo_slug, pr_number)?,
            Backend::Api(client) => client.get_pr_by_number(pr_number).await?,
        };

        self.persist_record(live)
    }

    pub async fn create_pr(
        &self,
        title: &str,
        body: Option<&str>,
        head: &str,
        base: &str,
        draft: bool,
    ) -> Result<PullRequestRecord, Error> {
        let live = match &self.backend {
            Backend::GhCli => gh_pr_create(
                &self.repo_path,
                &self.repo_slug,
                title,
                body,
                head,
                base,
                draft,
            )?,
            Backend::Api(client) => client.create_pr(title, body, head, base, draft).await?,
        };

        self.persist_record(live)
    }

    pub async fn update_pr(
        &self,
        pr_number: u64,
        base: Option<&str>,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<PullRequestRecord, Error> {
        let live = match &self.backend {
            Backend::GhCli => {
                gh_pr_edit(
                    &self.repo_path,
                    &self.repo_slug,
                    pr_number,
                    base,
                    title,
                    body,
                )?;
                gh_pr_view(&self.repo_path, &self.repo_slug, pr_number)?
            }
            Backend::Api(client) => client.update_pr(pr_number, base, title, body).await?,
        };

        self.persist_record(live)
    }

    pub async fn find_pr_by_head(
        &self,
        head_branch: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let live = match &self.backend {
            Backend::GhCli => gh_pr_find_by_head(&self.repo_path, &self.repo_slug, head_branch)?,
            Backend::Api(client) => client.find_pr_by_head_branch(head_branch).await?,
        };

        live.map(|record| self.persist_record(record)).transpose()
    }

    pub async fn find_pr_by_head_with_owner(
        &self,
        owner: &str,
        head_branch: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let live = match &self.backend {
            Backend::GhCli => {
                gh_pr_find_by_head_with_owner(&self.repo_path, &self.repo_slug, owner, head_branch)?
            }
            Backend::Api(client) => {
                client
                    .find_pr_by_head_branch_with_owner(owner, head_branch)
                    .await?
            }
        };

        live.map(|record| self.persist_record(record)).transpose()
    }

    pub fn mark_refreshed(&self, pr_number: u64) -> Result<Option<PullRequestRecord>, Error> {
        self.store.mark_refreshed(&self.repo_slug, pr_number)
    }

    fn persist_record(&self, record: PullRequestRecord) -> Result<PullRequestRecord, Error> {
        let persisted = self.store.upsert_record(&record)?;
        let _ = self
            .store
            .mark_refreshed(&self.repo_slug, persisted.pr_number)?;
        Ok(self
            .get_cached_pr(persisted.pr_number)?
            .unwrap_or(persisted))
    }
}

#[derive(Debug, Deserialize)]
struct GhPrViewResponse {
    number: u64,
    title: String,
    state: String,
    url: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
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
            "number,title,state,url,isDraft,baseRefName,headRefName,headRefOid,mergedAt",
        ],
    )?;
    let parsed: GhPrViewResponse =
        serde_json::from_str(&output).context("Failed to parse `gh pr view` JSON output")?;
    Ok(gh_response_to_record(repo_slug, parsed))
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
        args.push(String::new());
    }

    if draft {
        args.push("--draft".to_string());
    }

    let arg_refs: Vec<&str> = args.iter().map(|value| value.as_str()).collect();
    gh_output(repo_path, &arg_refs).context("`gh pr create` failed")?;

    gh_pr_find_by_head(repo_path, repo_slug, head)?
        .ok_or_else(|| anyhow::anyhow!("PR was created but could not be resolved by head branch"))
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

    let arg_refs: Vec<&str> = args.iter().map(|value| value.as_str()).collect();
    gh_output(repo_path, &arg_refs).context("`gh pr edit` failed")?;
    Ok(())
}

fn gh_pr_find_by_head(
    repo_path: &Path,
    repo_slug: &str,
    head_branch: &str,
) -> Result<Option<PullRequestRecord>, Error> {
    gh_pr_list(repo_path, repo_slug, head_branch)
}

fn gh_pr_find_by_head_with_owner(
    repo_path: &Path,
    repo_slug: &str,
    owner: &str,
    head_branch: &str,
) -> Result<Option<PullRequestRecord>, Error> {
    gh_pr_list(repo_path, repo_slug, &format!("{owner}:{head_branch}"))
}

fn gh_pr_list(
    repo_path: &Path,
    repo_slug: &str,
    head_selector: &str,
) -> Result<Option<PullRequestRecord>, Error> {
    let output = gh_output(
        repo_path,
        &[
            "pr",
            "list",
            "--repo",
            repo_slug,
            "--head",
            head_selector,
            "--state",
            "all",
            "--limit",
            "1",
            "--json",
            "number,title,state,url,isDraft,baseRefName,headRefName,headRefOid,mergedAt",
        ],
    )?;

    let parsed: Vec<GhPrViewResponse> =
        serde_json::from_str(&output).context("Failed to parse `gh pr list` JSON output")?;
    Ok(parsed
        .into_iter()
        .next()
        .map(|response| gh_response_to_record(repo_slug, response)))
}

fn gh_response_to_record(repo_slug: &str, response: GhPrViewResponse) -> PullRequestRecord {
    PullRequestRecord::from_snapshot(PullRequestSnapshot {
        repo_slug: repo_slug.to_string(),
        pr_number: response.number,
        title: response.title,
        url: response.url,
        base_ref: response.base_ref_name,
        head_ref: response.head_ref_name,
        head_sha: response.head_ref_oid,
        draft: response.is_draft,
        status: gh_state_to_pull_request_status(&response.state, response.merged_at.as_deref()),
    })
}

fn gh_state_to_pull_request_status(state: &str, merged_at: Option<&str>) -> PullRequestStatus {
    if merged_at.is_some() {
        PullRequestStatus::Merged
    } else if state.eq_ignore_ascii_case("closed") {
        PullRequestStatus::Closed
    } else {
        PullRequestStatus::Open
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
