use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("entry not found: {0}")]
    EntryNotFound(String),

    #[error("entry '{id}' has incompatible kind for this operation")]
    WrongEntryKind { id: String },

    #[error("AoB pattern for entry '{id}' produced no matches")]
    PatternNotFound { id: String },

    #[error("symbol '{0}' is not registered (enable its parent script first)")]
    SymbolNotRegistered(String),

    #[error("auto assembler error: {0}")]
    AutoAssembler(String),

    #[error("hex parse error: {0}")]
    HexParse(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("memory engine error: {0}")]
    Core(#[from] trainer_core::TrainerError),
}

pub type Result<T> = std::result::Result<T, SchemaError>;
