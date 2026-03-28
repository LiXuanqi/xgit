use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PullRequestStatus {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestRecord {
    pub repo_slug: String,
    pub pr_number: u64,
    pub title: String,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub head_sha: String,
    pub draft: bool,
    pub status: PullRequestStatus,
    pub branch_names: Vec<String>,
    pub remote_head_names: Vec<String>,
    pub commit_shas: Vec<String>,
    pub last_refreshed_at: Option<u64>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestSnapshot {
    pub repo_slug: String,
    pub pr_number: u64,
    pub title: String,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub head_sha: String,
    pub draft: bool,
    pub status: PullRequestStatus,
}

impl PullRequestRecord {
    pub fn from_snapshot(snapshot: PullRequestSnapshot) -> Self {
        let now = now_timestamp();
        Self {
            repo_slug: snapshot.repo_slug,
            pr_number: snapshot.pr_number,
            title: snapshot.title,
            url: snapshot.url,
            base_ref: snapshot.base_ref,
            head_ref: snapshot.head_ref,
            head_sha: snapshot.head_sha,
            draft: snapshot.draft,
            status: snapshot.status,
            branch_names: Vec::new(),
            remote_head_names: Vec::new(),
            commit_shas: Vec::new(),
            last_refreshed_at: Some(now),
            updated_at: now,
        }
    }

    pub fn is_closed_or_merged(&self) -> bool {
        matches!(
            self.status,
            PullRequestStatus::Closed | PullRequestStatus::Merged
        )
    }

    pub fn is_merged(&self) -> bool {
        matches!(self.status, PullRequestStatus::Merged)
    }

    pub fn is_fresh(&self, ttl_secs: u64) -> bool {
        let now = now_timestamp();
        self.last_refreshed_at
            .map(|refreshed_at| now.saturating_sub(refreshed_at) <= ttl_secs)
            .unwrap_or(false)
    }

    pub fn merge_with(&self, newer: &Self) -> Self {
        let mut merged = newer.clone();
        merged.branch_names = union_strings(&self.branch_names, &newer.branch_names);
        merged.remote_head_names = union_strings(&self.remote_head_names, &newer.remote_head_names);
        merged.commit_shas = union_strings(&self.commit_shas, &newer.commit_shas);
        merged.last_refreshed_at = newer.last_refreshed_at.or(self.last_refreshed_at);
        merged.updated_at = newer.updated_at.max(self.updated_at);
        merged
    }

    pub fn attach_branch_name(&mut self, branch_name: &str) -> bool {
        push_unique(&mut self.branch_names, branch_name)
    }

    pub fn attach_remote_head_name(&mut self, remote_head_name: &str) -> bool {
        push_unique(&mut self.remote_head_names, remote_head_name)
    }

    pub fn attach_commit_sha(&mut self, commit_sha: &str) -> bool {
        push_unique(&mut self.commit_shas, commit_sha)
    }

    pub fn mark_refreshed(&mut self) {
        let now = now_timestamp();
        self.last_refreshed_at = Some(now);
        self.updated_at = now;
    }

    pub fn touch(&mut self) {
        self.updated_at = now_timestamp();
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPullRequest {
    pub record: PullRequestRecord,
    pub is_stale: bool,
}

pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn push_unique(values: &mut Vec<String>, value: &str) -> bool {
    if values.iter().any(|existing| existing == value) {
        return false;
    }

    values.push(value.to_string());
    true
}

fn union_strings(existing: &[String], newer: &[String]) -> Vec<String> {
    let mut merged = existing.to_vec();
    for value in newer {
        push_unique(&mut merged, value);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::{PullRequestRecord, PullRequestSnapshot, PullRequestStatus};

    #[test]
    fn merge_with_preserves_associations() {
        let mut existing = PullRequestRecord::from_snapshot(PullRequestSnapshot {
            repo_slug: "owner/repo".to_string(),
            pr_number: 12,
            title: "First".to_string(),
            url: "https://example.com/12".to_string(),
            base_ref: "main".to_string(),
            head_ref: "feature".to_string(),
            head_sha: "aaa".to_string(),
            draft: false,
            status: PullRequestStatus::Open,
        });
        existing.attach_branch_name("feature");
        existing.attach_commit_sha("sha-1");

        let mut newer = PullRequestRecord::from_snapshot(PullRequestSnapshot {
            repo_slug: "owner/repo".to_string(),
            pr_number: 12,
            title: "Second".to_string(),
            url: "https://example.com/12".to_string(),
            base_ref: "main".to_string(),
            head_ref: "feature-renamed".to_string(),
            head_sha: "bbb".to_string(),
            draft: false,
            status: PullRequestStatus::Merged,
        });
        newer.attach_remote_head_name("feature-renamed");

        let merged = existing.merge_with(&newer);
        assert_eq!(merged.title, "Second");
        assert_eq!(merged.status, PullRequestStatus::Merged);
        assert!(merged.branch_names.iter().any(|branch| branch == "feature"));
        assert!(merged.commit_shas.iter().any(|sha| sha == "sha-1"));
        assert!(merged
            .remote_head_names
            .iter()
            .any(|remote_head| remote_head == "feature-renamed"));
    }
}
