use crate::git::GitRepo;
use crate::github::client::GitHubClient;
use anyhow::{Context, Error};
use console::style;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const TRAILER_KEY: &str = "XGit-PR";

#[derive(Debug, Clone)]
struct StackCommit {
    sha: String,
    subject: String,
    message: String,
    pr_number: Option<u64>,
}

#[derive(Debug, Clone)]
struct SyncRow {
    commit_short: String,
    pr_number: u64,
    head_branch: String,
    base_branch: String,
    url: String,
}

pub async fn handle_diff(repair: &Option<Vec<String>>) -> Result<(), Box<dyn std::error::Error>> {
    let repo = GitRepo::open(".")?;
    ensure_clean_worktree(&repo)?;

    let remote = detect_github_remote(&repo)?;
    let (owner, repo_name) = parse_github_url(&remote.url)?;
    let github_client = GitHubClient::new(owner, repo_name)?;
    let trunk_base = resolve_trunk_base_branch(&repo, &github_client).await?;
    let trunk_range = resolve_trunk_range_ref(&repo, &remote.name, &trunk_base)?;

    if let Some(repair_args) = repair {
        run_repair(&repo, &trunk_range, repair_args)?;
    }

    sync_stack(
        &repo,
        &github_client,
        &remote.name,
        &trunk_base,
        &trunk_range,
    )
    .await?;
    Ok(())
}

async fn sync_stack(
    repo: &GitRepo,
    github_client: &GitHubClient,
    remote_name: &str,
    trunk_base: &str,
    trunk_range: &str,
) -> Result<(), Error> {
    // Up to two passes: first may rewrite missing-trailer commits, second verifies and syncs.
    for _ in 0..2 {
        let stack = collect_stack(repo, trunk_range)?;
        if stack.is_empty() {
            println!(
                "{} No commits ahead of {}. Nothing to sync.",
                style("✓").green().bold(),
                style(trunk_base).cyan()
            );
            return Ok(());
        }

        validate_linear_stack(repo, &stack)?;
        validate_unique_pr_trailers(&stack)?;

        let missing_indices: Vec<usize> = stack
            .iter()
            .enumerate()
            .filter_map(|(idx, commit)| {
                if commit.pr_number.is_none() {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        if !missing_indices.is_empty() {
            create_prs_and_rewrite_missing_tip(
                repo,
                github_client,
                remote_name,
                trunk_base,
                trunk_range,
                &stack,
            )
            .await?;
            continue;
        }

        let rows = sync_existing_prs(repo, github_client, remote_name, trunk_base, &stack).await?;
        print_summary(&rows);
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Failed to stabilize stack after auto-rewrite. Please rerun xgit diff."
    ))
}

fn run_repair(repo: &GitRepo, trunk_range: &str, repair_args: &[String]) -> Result<(), Error> {
    if repair_args.len() != 2 {
        return Err(anyhow::anyhow!(
            "--repair requires exactly 2 values: <PR_NUMBER> <COMMIT_SHA>"
        ));
    }

    let pr_number: u64 = repair_args[0]
        .parse()
        .context("Invalid PR number provided to --repair")?;
    let target_sha = repair_args[1].trim().to_string();

    let stack = collect_stack(repo, trunk_range)?;
    let target_idx = stack
        .iter()
        .position(|c| c.sha.starts_with(&target_sha) || c.sha == target_sha)
        .ok_or_else(|| anyhow::anyhow!("Repair commit SHA is not in the current stack"))?;

    let base_ref = if target_idx == 0 {
        trunk_range.to_string()
    } else {
        stack[target_idx - 1].sha.clone()
    };

    let suffix = &stack[target_idx..];
    replay_suffix_with_optional_trailer(repo, &base_ref, suffix, Some((&suffix[0].sha, pr_number)))
        .context("Failed to apply repair rewrite")?;

    println!(
        "{} Repaired mapping for commit {} -> PR #{}",
        style("✓").green().bold(),
        style(short_sha(&target_sha)).cyan(),
        style(pr_number).cyan()
    );

    Ok(())
}

async fn create_prs_and_rewrite_missing_tip(
    repo: &GitRepo,
    github_client: &GitHubClient,
    remote_name: &str,
    trunk_base: &str,
    trunk_range: &str,
    stack: &[StackCommit],
) -> Result<(), Error> {
    let first_missing = stack
        .iter()
        .position(|c| c.pr_number.is_none())
        .ok_or_else(|| anyhow::anyhow!("No missing trailer found"))?;

    if stack[first_missing..].iter().any(|c| c.pr_number.is_some()) {
        return Err(anyhow::anyhow!(
            "Missing XGit-PR trailers must be contiguous at the tip. \
Run `git rebase -i {trunk_base}` and reword commits, then rerun xgit diff."
        ));
    }

    let missing_slice = &stack[first_missing..];
    let mut assigned: Vec<(String, u64)> = Vec::new();

    let mut base_branch = if first_missing == 0 {
        trunk_base.to_string()
    } else {
        let previous = stack[first_missing - 1]
            .pr_number
            .ok_or_else(|| anyhow::anyhow!("Previous commit is missing PR trailer"))?;
        pr_branch_name(previous)
    };

    for (idx, commit) in missing_slice.iter().enumerate() {
        let suffix = timestamp_suffix(idx as u64);
        let temp_branch = format!("xgit/new-{}-{suffix}", short_sha(&commit.sha));

        repo.force_push_commit_to_branch(remote_name, &commit.sha, &temp_branch)
            .context("Failed to push temporary PR head branch")?;

        let body = format!("Synced by xgit diff from commit {}", commit.sha);
        let created = github_client
            .create_pr(
                &commit.subject,
                Some(&body),
                &temp_branch,
                &base_branch,
                false,
            )
            .await
            .context("Failed to create PR for commit without trailer")?;

        let canonical = pr_branch_name(created.number);
        github_client
            .rename_branch(&temp_branch, &canonical)
            .await
            .context("Failed to rename PR head branch to canonical xgit/pr-* name")?;

        assigned.push((commit.sha.clone(), created.number));
        base_branch = canonical;
    }

    let base_ref = if first_missing == 0 {
        trunk_range.to_string()
    } else {
        stack[first_missing - 1].sha.clone()
    };

    replay_suffix_with_assigned_trailers(repo, &base_ref, missing_slice, &assigned)
        .context("Failed to rewrite missing-tip commits with PR trailers")?;

    Ok(())
}

async fn sync_existing_prs(
    repo: &GitRepo,
    github_client: &GitHubClient,
    remote_name: &str,
    trunk_base: &str,
    stack: &[StackCommit],
) -> Result<Vec<SyncRow>, Error> {
    let mut rows = Vec::new();
    let mut prev_pr_number: Option<u64> = None;

    for commit in stack {
        let pr_number = commit
            .pr_number
            .ok_or_else(|| anyhow::anyhow!("Expected trailer after rewrite pass"))?;
        let mut pr = github_client
            .get_pr_by_number(pr_number)
            .await
            .context(format!("Failed to fetch PR #{pr_number}"))?;

        if matches!(
            pr.state,
            crate::tui::branch_display::PullRequestState::Closed
        ) || pr.merged
        {
            return Err(anyhow::anyhow!(
                "Commit {} maps to PR #{} which is closed/merged. \
Repair this commit with `xgit diff --repair <pr_number> <commit_sha>`.",
                short_sha(&commit.sha),
                pr_number
            ));
        }

        let canonical_head = pr_branch_name(pr_number);
        if pr.head_ref != canonical_head {
            github_client
                .rename_branch(&pr.head_ref, &canonical_head)
                .await
                .context("Failed to normalize PR head branch name")?;
            pr = github_client
                .get_pr_by_number(pr_number)
                .await
                .context("Failed to refresh PR after head-branch rename")?;
        }

        repo.force_push_commit_to_branch(remote_name, &commit.sha, &canonical_head)
            .context("Failed to force-push commit to PR head branch")?;

        let expected_base = match prev_pr_number {
            Some(prev) => pr_branch_name(prev),
            None => trunk_base.to_string(),
        };

        if pr.base_ref != expected_base {
            pr = github_client
                .update_pr(pr_number, Some(&expected_base), None, None)
                .await
                .context("Failed to update PR base branch")?;
        }

        rows.push(SyncRow {
            commit_short: short_sha(&commit.sha),
            pr_number,
            head_branch: pr.head_ref.clone(),
            base_branch: pr.base_ref.clone(),
            url: pr.url.clone(),
        });

        prev_pr_number = Some(pr_number);
    }

    Ok(rows)
}

#[derive(Debug, Clone)]
struct GitHubRemote {
    name: String,
    url: String,
}

fn detect_github_remote(repo: &GitRepo) -> Result<GitHubRemote, Error> {
    for preferred in ["origin", "upstream"] {
        if let Ok(url) = repo.get_remote_url(preferred) {
            if url.contains("github.com") {
                return Ok(GitHubRemote {
                    name: preferred.to_string(),
                    url,
                });
            }
        }
    }

    let remotes = repo.get_remotes()?;
    for remote in remotes {
        if remote.url.contains("github.com") {
            return Ok(GitHubRemote {
                name: remote.name,
                url: remote.url,
            });
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

    Err(anyhow::anyhow!("Invalid GitHub URL format: {url}"))
}

async fn resolve_trunk_base_branch(
    repo: &GitRepo,
    github_client: &GitHubClient,
) -> Result<String, Error> {
    if let Ok(default_branch) = github_client.get_default_branch().await {
        return Ok(default_branch);
    }

    // Fallback for unusual API/auth failures: preserve previous local behavior.
    let branches = repo.get_all_branches()?;
    if branches.iter().any(|b| b == "main") {
        return Ok("main".to_string());
    }
    if branches.iter().any(|b| b == "master") {
        return Ok("master".to_string());
    }

    Err(anyhow::anyhow!(
        "Unable to determine trunk branch from GitHub default branch or local main/master"
    ))
}

fn resolve_trunk_range_ref(
    repo: &GitRepo,
    remote_name: &str,
    trunk_base: &str,
) -> Result<String, Error> {
    if let Err(fetch_err) = repo.fetch(remote_name, Some(trunk_base)) {
        print_fetch_debug(repo, remote_name, trunk_base, &fetch_err);
        return Err(fetch_err).context(format!(
            "Failed to fetch remote trunk branch '{remote_name}/{trunk_base}'"
        ));
    }

    let remote_ref = format!("{remote_name}/{trunk_base}");
    repo.list_commits_between(&remote_ref, "HEAD")
        .context(format!(
            "Remote-tracking trunk branch '{}' is unavailable after fetch",
            remote_ref
        ))?;
    Ok(remote_ref)
}

fn print_fetch_debug(repo: &GitRepo, remote_name: &str, trunk_base: &str, err: &Error) {
    let remote_ref = format!("{remote_name}/{trunk_base}");
    let remote_url = repo
        .get_remote_url(remote_name)
        .unwrap_or_else(|_| "<unknown remote url>".to_string());
    let current_branch = repo
        .get_current_branch()
        .unwrap_or_else(|_| "<detached-or-unknown>".to_string());

    let show_ref_result = git_output(
        repo.path(),
        &[
            "show-ref",
            "--verify",
            &format!("refs/remotes/{remote_ref}"),
        ],
    )
    .map(|_| "present".to_string())
    .unwrap_or_else(|e| format!("missing ({e})"));

    eprintln!(
        "{} debug: trunk fetch failed\n  remote: {} ({})\n  target branch: {}\n  remote-tracking ref: {} [{}]\n  current branch: {}\n  error chain: {:#}",
        style("⚠").yellow().bold(),
        style(remote_name).cyan(),
        style(remote_url).dim(),
        style(trunk_base).cyan(),
        style(&remote_ref).cyan(),
        style(show_ref_result).yellow(),
        style(current_branch).cyan(),
        err
    );
}

fn collect_stack(repo: &GitRepo, trunk_range_ref: &str) -> Result<Vec<StackCommit>, Error> {
    let commit_shas = repo.list_commits_between(trunk_range_ref, "HEAD")?;
    let mut stack = Vec::new();
    for sha in commit_shas {
        let message = repo.get_commit_message(&sha)?;
        let subject = repo.get_commit_subject(&sha)?;
        let pr_number = parse_pr_trailer(&message)?;
        stack.push(StackCommit {
            sha,
            subject,
            message,
            pr_number,
        });
    }
    Ok(stack)
}

fn validate_linear_stack(repo: &GitRepo, stack: &[StackCommit]) -> Result<(), Error> {
    for commit in stack {
        let parent_count = repo.get_commit_parent_count(&commit.sha)?;
        if parent_count > 1 {
            return Err(anyhow::anyhow!(
                "Merge commit {} found in stack. Only linear stacks are supported.",
                short_sha(&commit.sha)
            ));
        }
    }
    Ok(())
}

fn validate_unique_pr_trailers(stack: &[StackCommit]) -> Result<(), Error> {
    let mut seen = HashSet::new();
    for commit in stack {
        if let Some(pr_number) = commit.pr_number {
            if !seen.insert(pr_number) {
                return Err(anyhow::anyhow!(
                    "Duplicate {} trailer value #{} found in stack. \
Use `xgit diff --repair <pr_number> <commit_sha>` to fix mapping.",
                    TRAILER_KEY,
                    pr_number
                ));
            }
        }
    }
    Ok(())
}

fn parse_pr_trailer(message: &str) -> Result<Option<u64>, Error> {
    let mut found: Vec<u64> = Vec::new();
    for line in message.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{TRAILER_KEY}:")) {
            let value = rest.trim().trim_start_matches('#');
            if value.is_empty() {
                return Err(anyhow::anyhow!(
                    "Malformed {TRAILER_KEY} trailer: empty value"
                ));
            }
            let number = value
                .parse::<u64>()
                .context(format!("Malformed {TRAILER_KEY} trailer value: {value}"))?;
            found.push(number);
        }
    }

    match found.len() {
        0 => Ok(None),
        1 => Ok(found.first().copied()),
        _ => Err(anyhow::anyhow!(
            "Multiple {TRAILER_KEY} trailers found in one commit message"
        )),
    }
}

fn upsert_pr_trailer(message: &str, pr_number: u64) -> String {
    let mut kept_lines = Vec::new();
    for line in message.lines() {
        if line.trim().starts_with(&format!("{TRAILER_KEY}:")) {
            continue;
        }
        kept_lines.push(line);
    }

    while match kept_lines.last() {
        Some(line) => line.trim().is_empty(),
        None => false,
    } {
        kept_lines.pop();
    }

    let mut normalized = kept_lines.join("\n");
    if !normalized.is_empty() {
        normalized.push_str("\n\n");
    }
    normalized.push_str(&format!("{TRAILER_KEY}: #{pr_number}"));
    normalized.push('\n');
    normalized
}

fn replay_suffix_with_assigned_trailers(
    repo: &GitRepo,
    base_ref: &str,
    suffix: &[StackCommit],
    assigned: &[(String, u64)],
) -> Result<(), Error> {
    let mut lookup = std::collections::HashMap::new();
    for (sha, pr_number) in assigned {
        lookup.insert(sha.clone(), *pr_number);
    }
    replay_suffix_with_optional_trailer_lookup(repo, base_ref, suffix, &lookup)
}

fn replay_suffix_with_optional_trailer(
    repo: &GitRepo,
    base_ref: &str,
    suffix: &[StackCommit],
    override_target: Option<(&str, u64)>,
) -> Result<(), Error> {
    let mut lookup = std::collections::HashMap::new();
    if let Some((sha, pr_number)) = override_target {
        lookup.insert(sha.to_string(), pr_number);
    }
    replay_suffix_with_optional_trailer_lookup(repo, base_ref, suffix, &lookup)
}

fn replay_suffix_with_optional_trailer_lookup(
    repo: &GitRepo,
    base_ref: &str,
    suffix: &[StackCommit],
    override_lookup: &std::collections::HashMap<String, u64>,
) -> Result<(), Error> {
    run_git(repo.path(), &["reset", "--hard", base_ref])
        .context("Failed to reset to replay base")?;

    for commit in suffix {
        run_git(repo.path(), &["cherry-pick", &commit.sha]).context(format!(
            "Cherry-pick conflict while replaying commit {}. Resolve conflict and run `git cherry-pick --continue`, then rerun xgit diff.",
            short_sha(&commit.sha)
        ))?;

        let current_msg = git_output(repo.path(), &["log", "-1", "--format=%B"])?;
        let target_pr = override_lookup
            .get(&commit.sha)
            .copied()
            .or(commit.pr_number);
        if let Some(pr_number) = target_pr {
            let updated = upsert_pr_trailer(&current_msg, pr_number);
            amend_head_message(repo.path(), &updated)?;
        }
    }

    Ok(())
}

fn amend_head_message(repo_path: &std::path::Path, message: &str) -> Result<(), Error> {
    let temp_path: PathBuf =
        std::env::temp_dir().join(format!("xgit-pr-msg-{}.txt", timestamp_suffix(0)));
    fs::write(&temp_path, message).context("Failed to write temporary commit message")?;

    let temp_arg = temp_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid temporary commit message file path"))?
        .to_string();

    let amend_result = run_git(repo_path, &["commit", "--amend", "-F", &temp_arg]);
    let _ = fs::remove_file(&temp_path);
    amend_result.context("Failed to amend commit message with XGit-PR trailer")
}

fn ensure_clean_worktree(repo: &GitRepo) -> Result<(), Error> {
    if !repo.is_working_tree_clean()? {
        return Err(anyhow::anyhow!(
            "Working tree is not clean. Commit or stash your changes before running xgit diff."
        ));
    }
    Ok(())
}

fn pr_branch_name(pr_number: u64) -> String {
    format!("xgit/pr-{pr_number}")
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn timestamp_suffix(offset: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.saturating_add(offset)
}

fn run_git(repo_path: &std::path::Path, args: &[&str]) -> Result<(), Error> {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .status()
        .context("Failed to execute git command")?;

    if !status.success() {
        return Err(anyhow::anyhow!("git {:?} failed", args));
    }
    Ok(())
}

fn git_output(repo_path: &std::path::Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git command")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("git {:?} failed", args));
    }

    String::from_utf8(output.stdout).context("Invalid UTF-8 git output")
}

fn print_summary(rows: &[SyncRow]) {
    println!(
        "{} Synced {} stacked PRs",
        style("✓").green().bold(),
        style(rows.len()).cyan()
    );
    for row in rows {
        println!(
            "  {}  PR #{}  {} -> {}  {}",
            style(&row.commit_short).cyan(),
            style(row.pr_number).cyan().bold(),
            style(&row.head_branch).yellow(),
            style(&row.base_branch).yellow(),
            style(&row.url).dim()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_pr_trailer, upsert_pr_trailer};

    #[test]
    fn parse_pr_trailer_accepts_supported_formats() {
        let msg = "Title\n\nBody\n\nXGit-PR: #123\n";
        assert_eq!(parse_pr_trailer(msg).unwrap(), Some(123));

        let msg_no_hash = "Title\n\nXGit-PR: 456\n";
        assert_eq!(parse_pr_trailer(msg_no_hash).unwrap(), Some(456));
    }

    #[test]
    fn parse_pr_trailer_returns_none_when_absent() {
        assert_eq!(parse_pr_trailer("Title\n\nBody\n").unwrap(), None);
    }

    #[test]
    fn upsert_pr_trailer_replaces_existing_value() {
        let msg = "Title\n\nDetails\n\nXGit-PR: #3\n";
        let updated = upsert_pr_trailer(msg, 9);
        assert!(updated.contains("XGit-PR: #9"));
        assert!(!updated.contains("XGit-PR: #3"));
    }
}
