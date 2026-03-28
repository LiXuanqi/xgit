use crate::github::types::{now_timestamp, PullRequestRecord};
use anyhow::{Context, Error};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CURRENT_SCHEMA_VERSION: u32 = 1;

pub trait PrIndexStore: Send + Sync {
    fn get_by_pr(
        &self,
        repo_slug: &str,
        pr_number: u64,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn get_by_branch(
        &self,
        repo_slug: &str,
        branch_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn get_by_remote_head(
        &self,
        repo_slug: &str,
        remote_head_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn get_by_commit(
        &self,
        repo_slug: &str,
        commit_sha: &str,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn upsert_record(&self, record: &PullRequestRecord) -> Result<PullRequestRecord, Error>;
    fn attach_branch(
        &self,
        repo_slug: &str,
        pr_number: u64,
        branch_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn attach_remote_head(
        &self,
        repo_slug: &str,
        pr_number: u64,
        remote_head_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn attach_commit(
        &self,
        repo_slug: &str,
        pr_number: u64,
        commit_sha: &str,
    ) -> Result<Option<PullRequestRecord>, Error>;
    fn mark_refreshed(
        &self,
        repo_slug: &str,
        pr_number: u64,
    ) -> Result<Option<PullRequestRecord>, Error>;
}

#[derive(Debug, Clone)]
pub struct JsonPrIndexStore {
    path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PrIndexFile {
    version: u32,
    records: Vec<PullRequestRecord>,
}

impl JsonPrIndexStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_index(&self) -> Result<PrIndexFile, Error> {
        if !self.path.exists() {
            return Ok(PrIndexFile {
                version: CURRENT_SCHEMA_VERSION,
                records: Vec::new(),
            });
        }

        let contents = fs::read_to_string(&self.path)
            .context(format!("Failed to read PR index '{}'", self.path.display()))?;
        let index: PrIndexFile =
            serde_json::from_str(&contents).context("Failed to parse PR index JSON")?;

        if index.version != CURRENT_SCHEMA_VERSION {
            return Err(anyhow::anyhow!(
                "Unsupported PR index schema version {}",
                index.version
            ));
        }

        Ok(index)
    }

    fn save_index(&self, index: &PrIndexFile) -> Result<(), Error> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid PR index path"))?;
        fs::create_dir_all(parent).context(format!(
            "Failed to create PR index directory '{}'",
            parent.display()
        ))?;

        let temp_path = temp_path_for(&self.path);
        let payload =
            serde_json::to_vec_pretty(index).context("Failed to serialize PR index JSON")?;
        fs::write(&temp_path, payload).context(format!(
            "Failed to write temporary PR index '{}'",
            temp_path.display()
        ))?;
        fs::rename(&temp_path, &self.path).context(format!(
            "Failed to atomically replace PR index '{}'",
            self.path.display()
        ))?;
        Ok(())
    }

    fn mutate<F>(&self, mutator: F) -> Result<Option<PullRequestRecord>, Error>
    where
        F: FnOnce(&mut PrIndexFile) -> Result<Option<PullRequestRecord>, Error>,
    {
        let mut index = self.load_index()?;
        let result = mutator(&mut index)?;
        self.save_index(&index)?;
        Ok(result)
    }

    fn find_record<'a>(
        records: &'a [PullRequestRecord],
        repo_slug: &str,
        predicate: impl Fn(&'a PullRequestRecord) -> bool,
    ) -> Option<PullRequestRecord> {
        records
            .iter()
            .find(|record| record.repo_slug == repo_slug && predicate(record))
            .cloned()
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

impl PrIndexStore for JsonPrIndexStore {
    fn get_by_pr(
        &self,
        repo_slug: &str,
        pr_number: u64,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let index = self.load_index()?;
        Ok(Self::find_record(&index.records, repo_slug, |record| {
            record.pr_number == pr_number
        }))
    }

    fn get_by_branch(
        &self,
        repo_slug: &str,
        branch_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let index = self.load_index()?;
        Ok(Self::find_record(&index.records, repo_slug, |record| {
            record
                .branch_names
                .iter()
                .any(|branch| branch == branch_name)
        }))
    }

    fn get_by_remote_head(
        &self,
        repo_slug: &str,
        remote_head_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let index = self.load_index()?;
        Ok(Self::find_record(&index.records, repo_slug, |record| {
            record
                .remote_head_names
                .iter()
                .any(|remote_head| remote_head == remote_head_name)
        }))
    }

    fn get_by_commit(
        &self,
        repo_slug: &str,
        commit_sha: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        let index = self.load_index()?;
        Ok(Self::find_record(&index.records, repo_slug, |record| {
            record.commit_shas.iter().any(|sha| sha == commit_sha)
        }))
    }

    fn upsert_record(&self, record: &PullRequestRecord) -> Result<PullRequestRecord, Error> {
        let record = record.clone();
        self.mutate(|index| {
            if let Some(existing) = index.records.iter_mut().find(|existing| {
                existing.repo_slug == record.repo_slug && existing.pr_number == record.pr_number
            }) {
                *existing = existing.merge_with(&record);
                return Ok(Some(existing.clone()));
            }

            index.records.push(record.clone());
            Ok(Some(record))
        })?
        .ok_or_else(|| anyhow::anyhow!("Failed to upsert PR record"))
    }

    fn attach_branch(
        &self,
        repo_slug: &str,
        pr_number: u64,
        branch_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.mutate(|index| {
            let Some(record) = index
                .records
                .iter_mut()
                .find(|record| record.repo_slug == repo_slug && record.pr_number == pr_number)
            else {
                return Ok(None);
            };

            if record.attach_branch_name(branch_name) {
                record.touch();
            }
            Ok(Some(record.clone()))
        })
    }

    fn attach_remote_head(
        &self,
        repo_slug: &str,
        pr_number: u64,
        remote_head_name: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.mutate(|index| {
            let Some(record) = index
                .records
                .iter_mut()
                .find(|record| record.repo_slug == repo_slug && record.pr_number == pr_number)
            else {
                return Ok(None);
            };

            if record.attach_remote_head_name(remote_head_name) {
                record.touch();
            }
            Ok(Some(record.clone()))
        })
    }

    fn attach_commit(
        &self,
        repo_slug: &str,
        pr_number: u64,
        commit_sha: &str,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.mutate(|index| {
            let Some(record) = index
                .records
                .iter_mut()
                .find(|record| record.repo_slug == repo_slug && record.pr_number == pr_number)
            else {
                return Ok(None);
            };

            if record.attach_commit_sha(commit_sha) {
                record.touch();
            }
            Ok(Some(record.clone()))
        })
    }

    fn mark_refreshed(
        &self,
        repo_slug: &str,
        pr_number: u64,
    ) -> Result<Option<PullRequestRecord>, Error> {
        self.mutate(|index| {
            let Some(record) = index
                .records
                .iter_mut()
                .find(|record| record.repo_slug == repo_slug && record.pr_number == pr_number)
            else {
                return Ok(None);
            };

            record.mark_refreshed();
            Ok(Some(record.clone()))
        })
    }
}

fn temp_path_for(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pr-index.json");
    path.with_file_name(format!("{filename}.{}.tmp", now_timestamp()))
}

#[cfg(test)]
mod tests {
    use super::{JsonPrIndexStore, PrIndexFile, PrIndexStore, CURRENT_SCHEMA_VERSION};
    use crate::github::types::{PullRequestRecord, PullRequestSnapshot, PullRequestStatus};

    fn sample_record() -> PullRequestRecord {
        PullRequestRecord::from_snapshot(PullRequestSnapshot {
            repo_slug: "owner/repo".to_string(),
            pr_number: 42,
            title: "Feature".to_string(),
            url: "https://example.com/42".to_string(),
            base_ref: "main".to_string(),
            head_ref: "feature".to_string(),
            head_sha: "sha-1".to_string(),
            draft: false,
            status: PullRequestStatus::Open,
        })
    }

    #[test]
    fn json_store_round_trip_and_association_queries_work() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let store = JsonPrIndexStore::new(temp_dir.path().join("pr-index.json"));

        let mut record = sample_record();
        record.attach_branch_name("feature-local");
        store.upsert_record(&record).unwrap();
        store
            .attach_remote_head("owner/repo", 42, "feature")
            .unwrap();
        store.attach_commit("owner/repo", 42, "sha-2").unwrap();

        assert!(store.get_by_pr("owner/repo", 42).unwrap().is_some());
        assert!(store
            .get_by_branch("owner/repo", "feature-local")
            .unwrap()
            .is_some());
        assert!(store
            .get_by_remote_head("owner/repo", "feature")
            .unwrap()
            .is_some());
        assert!(store
            .get_by_commit("owner/repo", "sha-2")
            .unwrap()
            .is_some());
    }

    #[test]
    fn json_store_rejects_unsupported_schema_version() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let store = JsonPrIndexStore::new(temp_dir.path().join("pr-index.json"));

        let payload = PrIndexFile {
            version: CURRENT_SCHEMA_VERSION + 1,
            records: Vec::new(),
        };
        std::fs::write(store.path(), serde_json::to_vec(&payload).unwrap()).unwrap();

        let err = store.get_by_pr("owner/repo", 1).unwrap_err();
        assert!(err
            .to_string()
            .contains("Unsupported PR index schema version"));
    }

    #[test]
    fn json_store_overwrites_atomically_without_temp_file_leftovers() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let store = JsonPrIndexStore::new(temp_dir.path().join("pr-index.json"));

        store.upsert_record(&sample_record()).unwrap();

        let mut updated = sample_record();
        updated.title = "Updated title".to_string();
        updated.status = PullRequestStatus::Merged;
        store.upsert_record(&updated).unwrap();

        let saved = store.get_by_pr("owner/repo", 42).unwrap().unwrap();
        assert_eq!(saved.title, "Updated title");
        assert_eq!(saved.status, PullRequestStatus::Merged);

        let entries = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec!["pr-index.json".to_string()]);
    }
}
