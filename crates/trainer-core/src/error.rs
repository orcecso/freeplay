use thiserror::Error;

#[derive(Debug, Error)]
pub enum TrainerError {
    #[error("process not found: {0}")]
    ProcessNotFound(String),

    #[error("module not found: {0}")]
    ModuleNotFound(String),

    #[error("invalid AoB pattern: {0}")]
    InvalidPattern(String),

    #[error("pointer chain resolved to null at step {step}")]
    NullPointer { step: usize },

    #[error("type mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },

    #[error("patch already applied")]
    PatchAlreadyApplied,

    #[error("patch is not applied")]
    PatchNotApplied,

    #[error("windows API error in {context}: OS error {code}")]
    WindowsApi { context: &'static str, code: u32 },

    #[error("refused to attach: active anti-cheat detected ({0})")]
    AntiCheatDetected(String),

    #[error("unsupported platform: trainer-core currently requires Windows")]
    UnsupportedPlatform,
}

pub type Result<T> = std::result::Result<T, TrainerError>;
