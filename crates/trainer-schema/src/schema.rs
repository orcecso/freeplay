use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use trainer_core::{AobPattern, MemoryValue, PointerWidth, ValueType};

use crate::error::{Result, SchemaError};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainerManifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub game: GameTarget,
    pub author: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub entries: Vec<TrainerEntry>,
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameTarget {
    pub title: String,
    /// Target executable names (case-insensitive), e.g. `["game.exe", "game-win64-shipping.exe"]`.
    pub processes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    #[serde(default)]
    pub pointer_width: PointerWidthSchema,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PointerWidthSchema {
    #[serde(rename = "32")]
    Bits32,
    #[default]
    #[serde(rename = "64")]
    Bits64,
}

impl From<PointerWidthSchema> for PointerWidth {
    fn from(value: PointerWidthSchema) -> Self {
        match value {
            PointerWidthSchema::Bits32 => PointerWidth::Bits32,
            PointerWidthSchema::Bits64 => PointerWidth::Bits64,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValueTypeSchema {
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl From<ValueTypeSchema> for ValueType {
    fn from(v: ValueTypeSchema) -> Self {
        match v {
            ValueTypeSchema::U8 => ValueType::U8,
            ValueTypeSchema::I16 => ValueType::I16,
            ValueTypeSchema::U16 => ValueType::U16,
            ValueTypeSchema::I32 => ValueType::I32,
            ValueTypeSchema::U32 => ValueType::U32,
            ValueTypeSchema::I64 => ValueType::I64,
            ValueTypeSchema::U64 => ValueType::U64,
            ValueTypeSchema::F32 => ValueType::F32,
            ValueTypeSchema::F64 => ValueType::F64,
        }
    }
}

/// JSON-friendly numeric value representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ScalarValue {
    Int(i64),
    Float(f64),
}

impl ScalarValue {
    pub fn to_memory_value(&self, ty: ValueTypeSchema) -> MemoryValue {
        match (self, ty) {
            (ScalarValue::Int(v), ValueTypeSchema::U8) => MemoryValue::U8(*v as u8),
            (ScalarValue::Int(v), ValueTypeSchema::I16) => MemoryValue::I16(*v as i16),
            (ScalarValue::Int(v), ValueTypeSchema::U16) => MemoryValue::U16(*v as u16),
            (ScalarValue::Int(v), ValueTypeSchema::I32) => MemoryValue::I32(*v as i32),
            (ScalarValue::Int(v), ValueTypeSchema::U32) => MemoryValue::U32(*v as u32),
            (ScalarValue::Int(v), ValueTypeSchema::I64) => MemoryValue::I64(*v),
            (ScalarValue::Int(v), ValueTypeSchema::U64) => MemoryValue::U64(*v as u64),
            (ScalarValue::Int(v), ValueTypeSchema::F32) => MemoryValue::F32(*v as f32),
            (ScalarValue::Int(v), ValueTypeSchema::F64) => MemoryValue::F64(*v as f64),
            (ScalarValue::Float(v), ValueTypeSchema::U8) => MemoryValue::U8(*v as u8),
            (ScalarValue::Float(v), ValueTypeSchema::I16) => MemoryValue::I16(*v as i16),
            (ScalarValue::Float(v), ValueTypeSchema::U16) => MemoryValue::U16(*v as u16),
            (ScalarValue::Float(v), ValueTypeSchema::I32) => MemoryValue::I32(*v as i32),
            (ScalarValue::Float(v), ValueTypeSchema::U32) => MemoryValue::U32(*v as u32),
            (ScalarValue::Float(v), ValueTypeSchema::I64) => MemoryValue::I64(*v as i64),
            (ScalarValue::Float(v), ValueTypeSchema::U64) => MemoryValue::U64(*v as u64),
            (ScalarValue::Float(v), ValueTypeSchema::F32) => MemoryValue::F32(*v as f32),
            (ScalarValue::Float(v), ValueTypeSchema::F64) => MemoryValue::F64(*v),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainerEntry {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(flatten)]
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryKind {
    /// Reads, writes, or freezes a typed value at an address/pointer chain/AoB offset.
    Value {
        address: AddressSpec,
        value_type: ValueTypeSchema,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<ScalarValue>,
        #[serde(default)]
        freeze: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    /// Reversible byte patch located by an AoB signature (no arbitrary code execution).
    AobPatch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        module: Option<String>,
        pattern: String,
        #[serde(default)]
        patch_offset: isize,
        /// Space-separated hex bytes to write, e.g. `"90 90 90 90"`.
        patch_bytes: String,
        #[serde(default = "default_true")]
        executable_only: bool,
    },
    /// Opt-in Cheat Engine Auto Assembler x64 code-cave script (no Lua).
    AutoAssembler {
        script: String,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AddressSpec {
    /// `module_base(module) + base_offset`, then dereference each offset in top-to-bottom order.
    ModulePointerChain {
        module: String,
        /// Hex string like `"0x1A2B3C"` or `"1A2B3C"`.
        base_offset: String,
        /// Signed hex offsets like `["0x10", "0x28", "-0x4"]`.
        #[serde(default)]
        offsets: Vec<String>,
    },
    /// Absolute address (hex string) with optional pointer chain offsets.
    Absolute {
        address: String,
        #[serde(default)]
        offsets: Vec<String>,
    },
    /// Locate an address via AoB signature + signed byte offset from the match start.
    AobOffset {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        module: Option<String>,
        pattern: String,
        #[serde(default)]
        offset: isize,
        #[serde(default)]
        executable_only: bool,
    },
    /// Address defined dynamically by an active Auto Assembler script (`registersymbol`).
    Symbol {
        symbol: String,
        #[serde(default)]
        offsets: Vec<String>,
    },
}

pub fn parse_hex_usize(raw: &str) -> Result<usize> {
    let trimmed = raw.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    usize::from_str_radix(hex, 16)
        .map_err(|_| SchemaError::HexParse(format!("invalid hex unsigned value '{raw}'")))
}

pub fn parse_hex_isize(raw: &str) -> Result<isize> {
    let trimmed = raw.trim();
    let (neg, rest) = if let Some(r) = trimmed.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = trimmed.strip_prefix('+') {
        (false, r)
    } else {
        (false, trimmed)
    };
    let mag = parse_hex_usize(rest)?;
    let signed = isize::try_from(mag)
        .map_err(|_| SchemaError::HexParse(format!("offset overflow '{raw}'")))?;
    Ok(if neg { -signed } else { signed })
}

pub fn parse_hex_bytes(raw: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for tok in raw.split_whitespace() {
        let hex = tok
            .strip_prefix("0x")
            .or_else(|| tok.strip_prefix("0X"))
            .unwrap_or(tok);
        if hex.len() != 2 {
            return Err(SchemaError::HexParse(format!(
                "patch byte '{tok}' must be 2 hex digits"
            )));
        }
        let b = u8::from_str_radix(hex, 16)
            .map_err(|_| SchemaError::HexParse(format!("invalid hex byte '{tok}'")))?;
        out.push(b);
    }
    if out.is_empty() {
        return Err(SchemaError::HexParse("patch_bytes cannot be empty".into()));
    }
    Ok(out)
}

impl TrainerManifest {
    pub fn from_json_str(s: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_str(s)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json_str(&content)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()?;
        let json = self.to_json_pretty()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(SchemaError::Validation(format!(
                "unsupported schema_version {}, expected {}",
                self.schema_version, CURRENT_SCHEMA_VERSION
            )));
        }
        if self.id.trim().is_empty() {
            return Err(SchemaError::Validation("manifest id cannot be empty".into()));
        }
        if self.game.title.trim().is_empty() {
            return Err(SchemaError::Validation("game.title cannot be empty".into()));
        }
        if self.game.processes.is_empty()
            || self.game.processes.iter().any(|p| p.trim().is_empty())
        {
            return Err(SchemaError::Validation(
                "game.processes must contain at least one non-empty executable name".into(),
            ));
        }

        let mut seen_ids = HashSet::new();
        for entry in &self.entries {
            if entry.id.trim().is_empty() {
                return Err(SchemaError::Validation("entry id cannot be empty".into()));
            }
            if !seen_ids.insert(entry.id.as_str()) {
                return Err(SchemaError::Validation(format!(
                    "duplicate entry id '{}'",
                    entry.id
                )));
            }
            if entry.label.trim().is_empty() {
                return Err(SchemaError::Validation(format!(
                    "entry '{}' label cannot be empty",
                    entry.id
                )));
            }

            match &entry.kind {
                EntryKind::Value { address, .. } => validate_address_spec(address)?,
                EntryKind::AobPatch {
                    pattern,
                    patch_bytes,
                    ..
                } => {
                    AobPattern::parse(pattern)?;
                    parse_hex_bytes(patch_bytes)?;
                }
                EntryKind::AutoAssembler { script } => {
                    if script.trim().is_empty() {
                        return Err(SchemaError::Validation(format!(
                            "entry '{}' AutoAssembler script cannot be empty",
                            entry.id
                        )));
                    }
                    let lower = script.to_ascii_lowercase();
                    if lower.contains("{$lua}") || lower.contains("luacall(") {
                        return Err(SchemaError::Validation(format!(
                            "entry '{}' contains Lua which is not permitted",
                            entry.id
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn find_entry(&self, id: &str) -> Option<&TrainerEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

fn validate_address_spec(spec: &AddressSpec) -> Result<()> {
    match spec {
        AddressSpec::ModulePointerChain {
            module,
            base_offset,
            offsets,
        } => {
            if module.trim().is_empty() {
                return Err(SchemaError::Validation(
                    "module name cannot be empty".into(),
                ));
            }
            parse_hex_usize(base_offset)?;
            for o in offsets {
                parse_hex_isize(o)?;
            }
        }
        AddressSpec::Absolute { address, offsets } => {
            parse_hex_usize(address)?;
            for o in offsets {
                parse_hex_isize(o)?;
            }
        }
        AddressSpec::AobOffset { pattern, .. } => {
            AobPattern::parse(pattern)?;
        }
        AddressSpec::Symbol { symbol, offsets } => {
            if symbol.trim().is_empty() {
                return Err(SchemaError::Validation("symbol name cannot be empty".into()));
            }
            for o in offsets {
                parse_hex_isize(o)?;
            }
        }
    }
    Ok(())
}
