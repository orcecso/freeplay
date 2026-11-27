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

    #[error("linux API error in {context}: errno {code}")]
    LinuxApi { context: &'static str, code: i32 },

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("refused to attach: active anti-cheat detected ({0})")]
    AntiCheatDetected(String),

    #[error("unsupported operation on this platform: {0}")]
    UnsupportedOperation(String),

    #[error("unsupported platform: trainer-core supports Windows and Linux")]
    UnsupportedPlatform,
}

pub type Result<T> = std::result::Result<T, TrainerError>;
