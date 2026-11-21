//! `trainer-repo` — Tachiyomi-style community repository catalog, fetcher,
//! SHA-256 integrity verifier, and local trainer store.

pub mod error;
pub mod fetch;
pub mod index;
pub mod manager;

pub use error::{RepoError, Result};
pub use index::{
    compute_sha256_hex, resolve_trainer_url, RepoIndex, RepoTrainerItem, CURRENT_INDEX_VERSION,
};
pub use manager::{AvailableUpdate, CatalogHit, InstalledRecord, RepoManager, RepoSource};
