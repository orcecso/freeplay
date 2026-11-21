use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("repository not found: {0}")]
    RepoNotFound(String),

    #[error("repository already exists: {0}")]
    RepoAlreadyExists(String),

    #[error("trainer '{trainer_id}' not found in repository '{repo_id}'")]
    TrainerNotFoundInRepo { repo_id: String, trainer_id: String },

    #[error("installed trainer not found: {0}")]
    InstalledNotFound(String),

    #[error("SHA-256 checksum mismatch for '{trainer_id}': expected {expected}, got {actual}")]
    ChecksumMismatch {
        trainer_id: String,
        expected: String,
        actual: String,
    },

    #[error("fetch failed for '{url}': {reason}")]
    FetchFailed { url: String, reason: String },

    #[error("payload from '{url}' exceeded maximum size of {max_bytes} bytes")]
    PayloadTooLarge { url: String, max_bytes: usize },

    #[error("invalid path traversal in '{0}'")]
    InvalidPath(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("schema error: {0}")]
    Schema(#[from] trainer_schema::SchemaError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RepoError>;
