//! `trainer-core` — User-mode memory inspection, pointer-chain resolution,
//! AoB signature scanning, reversible byte patching, and value freezing
//! for single-player games.

pub mod aob;
pub mod error;
pub mod freezer;
pub mod guard;
#[cfg(target_os = "linux")]
pub(crate) mod linux;
pub mod memory;
pub mod patch;
pub mod process;
pub mod scanner;

pub use aob::{AobPattern, ScanOptions};
pub use error::{Result, TrainerError};
pub use freezer::{FreezeManager, FreezeTarget};
pub use memory::{MemoryValue, PointerWidth, ValueType};
pub use patch::BytePatch;
pub use process::{find_processes_by_name, list_processes, ModuleInfo, ProcessHandle, ProcessInfo};
pub use scanner::{resolve_static_module, MemoryScanner, ScanFilter, ScanHit, ScanProgress};
