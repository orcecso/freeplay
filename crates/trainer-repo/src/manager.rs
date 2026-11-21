use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use trainer_schema::TrainerManifest;

use crate::error::{Result, RepoError};
use crate::fetch::{fetch_bytes, fetch_text};
use crate::index::{
    compute_sha256_hex, resolve_trainer_url, validate_safe_id, RepoIndex, RepoTrainerItem,
};

/// A user-configured community repository source (stored in `repos.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoSource {
    pub repo_id: String,
    pub repo_name: String,
    pub index_url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Metadata tracking an installed trainer in `installed/state.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledRecord {
    pub trainer_id: String,
    pub title: String,
    pub version: String,
    pub author: String,
    pub processes: Vec<String>,
    /// `None` if imported locally (e.g. from a `.CT` file or local JSON).
    pub source_repo_id: Option<String>,
    pub sha256: String,
}

/// Search hit returned by `RepoManager::search_catalog`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogHit {
    pub repo_id: String,
    pub repo_name: String,
    pub item: RepoTrainerItem,
    pub installed_version: Option<String>,
}

/// Update notification returned by `RepoManager::check_updates`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub trainer_id: String,
    pub repo_id: String,
    pub title: String,
    pub installed_version: String,
    pub available_version: String,
}

/// Manages community repositories, cached catalogs, and locally installed trainers.
pub struct RepoManager {
    root_dir: PathBuf,
}

impl RepoManager {
    /// Opens (or initializes) a local trainer store at `root_dir`.
    pub fn open(root_dir: impl AsRef<Path>) -> Result<Self> {
        let root_dir = root_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(root_dir.join("cache"))?;
        std::fs::create_dir_all(root_dir.join("installed"))?;
        Ok(Self { root_dir })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    fn repos_file(&self) -> PathBuf {
        self.root_dir.join("repos.json")
    }

    fn cache_file(&self, repo_id: &str) -> PathBuf {
        self.root_dir.join("cache").join(format!("{repo_id}.index.json"))
    }

    fn installed_state_file(&self) -> PathBuf {
        self.root_dir.join("installed").join("state.json")
    }

    fn installed_manifest_file(&self, trainer_id: &str) -> PathBuf {
        self.root_dir
            .join("installed")
            .join(format!("{trainer_id}.trainer.json"))
    }

    pub fn list_repos(&self) -> Result<Vec<RepoSource>> {
        let path = self.repos_file();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_repos(&self, repos: &[RepoSource]) -> Result<()> {
        let json = serde_json::to_string_pretty(repos)?;
        std::fs::write(self.repos_file(), json)?;
        Ok(())
    }

    fn load_installed_map(&self) -> Result<HashMap<String, InstalledRecord>> {
        let path = self.installed_state_file();
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_installed_map(&self, map: &HashMap<String, InstalledRecord>) -> Result<()> {
        let json = serde_json::to_string_pretty(map)?;
        std::fs::write(self.installed_state_file(), json)?;
        Ok(())
    }

    /// Fetches `index_url`, validates the `RepoIndex`, registers the source, and caches its catalog.
    pub fn add_repo(&self, index_url: &str) -> Result<RepoIndex> {
        let raw = fetch_text(index_url)?;
        let index = RepoIndex::from_json_str(&raw)?;

        let mut repos = self.list_repos()?;
        if repos.iter().any(|r| r.repo_id == index.repo_id) {
            return Err(RepoError::RepoAlreadyExists(index.repo_id));
        }

        repos.push(RepoSource {
            repo_id: index.repo_id.clone(),
            repo_name: index.repo_name.clone(),
            index_url: index_url.to_string(),
            enabled: true,
        });
        self.save_repos(&repos)?;
        std::fs::write(self.cache_file(&index.repo_id), raw)?;
        Ok(index)
    }

    /// Removes a repository source and its cached catalog (installed trainers remain until uninstalled).
    pub fn remove_repo(&self, repo_id: &str) -> Result<()> {
        let mut repos = self.list_repos()?;
        let before = repos.len();
        repos.retain(|r| r.repo_id != repo_id);
        if repos.len() == before {
            return Err(RepoError::RepoNotFound(repo_id.into()));
        }
        self.save_repos(&repos)?;
        let cache = self.cache_file(repo_id);
        if cache.exists() {
            let _ = std::fs::remove_file(cache);
        }
        Ok(())
    }

    /// Re-fetches and caches the `index.json` for a single repository.
    pub fn refresh_repo(&self, repo_id: &str) -> Result<RepoIndex> {
        let mut repos = self.list_repos()?;
        let source = repos
            .iter_mut()
            .find(|r| r.repo_id == repo_id)
            .ok_or_else(|| RepoError::RepoNotFound(repo_id.into()))?;

        let raw = fetch_text(&source.index_url)?;
        let index = RepoIndex::from_json_str(&raw)?;
        source.repo_name = index.repo_name.clone();
        self.save_repos(&repos)?;
        std::fs::write(self.cache_file(repo_id), raw)?;
        Ok(index)
    }

    /// Re-fetches all enabled repositories. Returns `(succeeded, errors)` so one offline repo
    /// does not block others.
    pub fn refresh_all(&self) -> Result<(Vec<RepoIndex>, Vec<(String, RepoError)>)> {
        let repos = self.list_repos()?;
        let mut ok = Vec::new();
        let mut errs = Vec::new();
        for repo in repos.into_iter().filter(|r| r.enabled) {
            match self.refresh_repo(&repo.repo_id) {
                Ok(idx) => ok.push(idx),
                Err(e) => errs.push((repo.repo_id, e)),
            }
        }
        Ok((ok, errs))
    }

    /// Loads the cached `RepoIndex` for a given repository.
    pub fn load_cached_index(&self, repo_id: &str) -> Result<RepoIndex> {
        let path = self.cache_file(repo_id);
        if !path.exists() {
            return Err(RepoError::RepoNotFound(repo_id.into()));
        }
        let raw = std::fs::read_to_string(path)?;
        RepoIndex::from_json_str(&raw)
    }

    /// Searches all enabled cached repositories by title, process name, trainer ID, or Steam AppID.
    /// An empty `query` returns all trainers across all enabled repos.
    pub fn search_catalog(&self, query: &str) -> Result<Vec<CatalogHit>> {
        let q = query.trim().to_ascii_lowercase();
        let repos = self.list_repos()?;
        let installed = self.load_installed_map()?;
        let mut hits = Vec::new();

        for repo in repos.into_iter().filter(|r| r.enabled) {
            let Ok(index) = self.load_cached_index(&repo.repo_id) else {
                continue;
            };
            for item in index.trainers {
                let matches = q.is_empty()
                    || item.title.to_ascii_lowercase().contains(&q)
                    || item.id.to_ascii_lowercase().contains(&q)
                    || item
                        .processes
                        .iter()
                        .any(|p| p.to_ascii_lowercase().contains(&q))
                    || item
                        .steam_app_id
                        .is_some_and(|app_id| app_id.to_string() == q);

                if matches {
                    let installed_version =
                        installed.get(&item.id).map(|rec| rec.version.clone());
                    hits.push(CatalogHit {
                        repo_id: repo.repo_id.clone(),
                        repo_name: repo.repo_name.clone(),
                        item,
                        installed_version,
                    });
                }
            }
        }
        Ok(hits)
    }

    /// Downloads, SHA-256 verifies (if digest present), validates, and installs a trainer from a repo.
    pub fn install_trainer(&self, repo_id: &str, trainer_id: &str) -> Result<InstalledRecord> {
        let repos = self.list_repos()?;
        let source = repos
            .iter()
            .find(|r| r.repo_id == repo_id)
            .ok_or_else(|| RepoError::RepoNotFound(repo_id.into()))?;

        let index = self.load_cached_index(repo_id)?;
        let item = index
            .find_trainer(trainer_id)
            .ok_or_else(|| RepoError::TrainerNotFoundInRepo {
                repo_id: repo_id.into(),
                trainer_id: trainer_id.into(),
            })?;

        let trainer_url = resolve_trainer_url(&source.index_url, &item.path)?;
        let raw_bytes = fetch_bytes(&trainer_url)?;
        let actual_sha = compute_sha256_hex(&raw_bytes);

        if let Some(expected) = &item.sha256 {
            let expected_lower = expected.trim().to_ascii_lowercase();
            if actual_sha != expected_lower {
                return Err(RepoError::ChecksumMismatch {
                    trainer_id: trainer_id.into(),
                    expected: expected_lower,
                    actual: actual_sha,
                });
            }
        }

        let raw_text = String::from_utf8(raw_bytes).map_err(|e| RepoError::FetchFailed {
            url: trainer_url,
            reason: format!("trainer JSON is not valid UTF-8: {e}"),
        })?;
        let manifest = TrainerManifest::from_json_str(&raw_text)?;
        validate_safe_id(&manifest.id, "manifest.id")?;

        std::fs::write(self.installed_manifest_file(&manifest.id), &raw_text)?;

        let record = InstalledRecord {
            trainer_id: manifest.id.clone(),
            title: manifest.game.title.clone(),
            version: item.trainer_version.clone(),
            author: manifest.author.clone(),
            processes: manifest.game.processes.clone(),
            source_repo_id: Some(repo_id.to_string()),
            sha256: actual_sha,
        };

        let mut map = self.load_installed_map()?;
        map.insert(record.trainer_id.clone(), record.clone());
        self.save_installed_map(&map)?;
        Ok(record)
    }

    /// Saves a locally created or `.CT`-imported `TrainerManifest` into the installed library.
    pub fn import_local_manifest(&self, manifest: &TrainerManifest) -> Result<InstalledRecord> {
        manifest.validate()?;
        validate_safe_id(&manifest.id, "manifest.id")?;
        let json = manifest.to_json_pretty()?;
        let sha = compute_sha256_hex(json.as_bytes());

        std::fs::write(self.installed_manifest_file(&manifest.id), &json)?;

        let record = InstalledRecord {
            trainer_id: manifest.id.clone(),
            title: manifest.game.title.clone(),
            version: manifest.version.clone(),
            author: manifest.author.clone(),
            processes: manifest.game.processes.clone(),
            source_repo_id: None,
            sha256: sha,
        };

        let mut map = self.load_installed_map()?;
        map.insert(record.trainer_id.clone(), record.clone());
        self.save_installed_map(&map)?;
        Ok(record)
    }

    pub fn uninstall_trainer(&self, trainer_id: &str) -> Result<()> {
        let mut map = self.load_installed_map()?;
        if map.remove(trainer_id).is_none() {
            return Err(RepoError::InstalledNotFound(trainer_id.into()));
        }
        self.save_installed_map(&map)?;
        let file = self.installed_manifest_file(trainer_id);
        if file.exists() {
            let _ = std::fs::remove_file(file);
        }
        Ok(())
    }

    pub fn list_installed(&self) -> Result<Vec<InstalledRecord>> {
        let mut items: Vec<InstalledRecord> = self.load_installed_map()?.into_values().collect();
        items.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(items)
    }

    pub fn load_installed(&self, trainer_id: &str) -> Result<TrainerManifest> {
        validate_safe_id(trainer_id, "trainer_id")?;
        let path = self.installed_manifest_file(trainer_id);
        if !path.exists() {
            return Err(RepoError::InstalledNotFound(trainer_id.into()));
        }
        Ok(TrainerManifest::load_from_file(path)?)
    }

    /// Compares installed repo-backed trainers against cached repo catalogs to detect updates.
    pub fn check_updates(&self) -> Result<Vec<AvailableUpdate>> {
        let installed = self.load_installed_map()?;
        let mut updates = Vec::new();

        for rec in installed.values() {
            let Some(repo_id) = &rec.source_repo_id else {
                continue;
            };
            let Ok(index) = self.load_cached_index(repo_id) else {
                continue;
            };
            if let Some(item) = index.find_trainer(&rec.trainer_id) {
                if item.trainer_version != rec.version {
                    updates.push(AvailableUpdate {
                        trainer_id: rec.trainer_id.clone(),
                        repo_id: repo_id.clone(),
                        title: item.title.clone(),
                        installed_version: rec.version.clone(),
                        available_version: item.trainer_version.clone(),
                    });
                }
            }
        }
        Ok(updates)
    }
}
