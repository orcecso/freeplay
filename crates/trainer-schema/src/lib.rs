//! `trainer-schema` — Declarative JSON trainer manifest format,
//! Cheat Engine `.CT` XML importer/exporter, and runtime session bridge.

pub mod aa;
pub mod ct;
pub mod error;
pub mod runtime;
pub mod schema;

pub use aa::{activate_auto_assembler, ActiveAutoAssembler};

pub use ct::{export_ct_xml, import_ct_xml, ImportOptions, ImportResult, SkippedEntry};
pub use error::{Result, SchemaError};
pub use runtime::TrainerSession;
pub use schema::{
    AddressSpec, EntryKind, GameTarget, PointerWidthSchema, ScalarValue, TrainerEntry,
    TrainerManifest, ValueTypeSchema, CURRENT_SCHEMA_VERSION,
};
