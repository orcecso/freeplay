use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, RepoError};

pub const CURRENT_INDEX_VERSION: u32 = 1;

/// Catalog file (`index.json`) hosted by a community trainer repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoIndex {
    #[serde(default = "default_index_version")]
    pub schema_version: u32,
    pub repo_id: String,
    pub repo_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub trainers: Vec<RepoTrainerItem>,
}

fn default_index_version() -> u32 {
    CURRENT_INDEX_VERSION
}

/// Single trainer entry advertised inside a repository's `index.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoTrainerItem {
    pub id: String,
    pub title: String,
    pub processes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    pub trainer_version: String,
    pub author: String,
    /// Relative path from `index.json` (e.g. `"trainers/my-game.trainer.json"`) or full `http(s)://` URL.
    pub path: String,
    /// Optional hex-encoded SHA-256 digest of the trainer JSON file for integrity verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl RepoIndex {
    pub fn from_json_str(s: &str) -> Result<Self> {
        let idx: Self = serde_json::from_str(s)?;
        idx.validate()?;
        Ok(idx)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CURRENT_INDEX_VERSION {
            return Err(RepoError::Validation(format!(
                "unsupported index schema_version {}, expected {}",
                self.schema_version, CURRENT_INDEX_VERSION
            )));
        }
        validate_safe_id(&self.repo_id, "repo_id")?;
        if self.repo_name.trim().is_empty() {
            return Err(RepoError::Validation("repo_name cannot be empty".into()));
        }

        let mut seen = HashSet::new();
        for item in &self.trainers {
            validate_safe_id(&item.id, "trainer id")?;
            if !seen.insert(item.id.as_str()) {
                return Err(RepoError::Validation(format!(
                    "duplicate trainer id '{}' in repo '{}'",
                    item.id, self.repo_id
                )));
            }
            if item.title.trim().is_empty() {
                return Err(RepoError::Validation(format!(
                    "trainer '{}' title cannot be empty",
                    item.id
                )));
            }
            if item.processes.is_empty() || item.processes.iter().any(|p| p.trim().is_empty()) {
                return Err(RepoError::Validation(format!(
                    "trainer '{}' must specify at least one process",
                    item.id
                )));
            }
            if item.path.trim().is_empty() {
                return Err(RepoError::Validation(format!(
                    "trainer '{}' path cannot be empty",
                    item.id
                )));
            }
            if item.path.contains("..") {
                return Err(RepoError::InvalidPath(item.path.clone()));
            }
            if let Some(hash) = &item.sha256 {
                let h = hash.trim();
                if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(RepoError::Validation(format!(
                        "trainer '{}' has invalid sha256 digest",
                        item.id
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn find_trainer(&self, trainer_id: &str) -> Option<&RepoTrainerItem> {
        self.trainers.iter().find(|t| t.id == trainer_id)
    }
}

/// Ensures an ID is safe for use as a local filename (alphanumeric, `-`, `_`, `.`).
pub fn validate_safe_id(id: &str, field: &str) -> Result<()> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || trimmed.contains("..")
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(RepoError::Validation(format!(
            "invalid {field} '{id}': must use only [A-Za-z0-9._-]"
        )));
    }
    Ok(())
}

/// Computes lowercase hex SHA-256 of raw bytes.
pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Resolves a trainer `item.path` relative to the repository's `index_url`.
pub fn resolve_trainer_url(index_url: &str, item_path: &str) -> Result<String> {
    if item_path.contains("..") {
        return Err(RepoError::InvalidPath(item_path.into()));
    }
    if item_path.starts_with("https://") || item_path.starts_with("http://") {
        return Ok(item_path.to_string());
    }

    let rel = item_path.trim_start_matches(['/', '\\']);
    if let Some((dir, _)) = index_url.rsplit_once('/') {
        Ok(format!("{dir}/{rel}"))
    } else if let Some((dir, _)) = index_url.rsplit_once('\\') {
        Ok(format!("{dir}\\{rel}"))
    } else {
        Ok(rel.to_string())
    }
}
