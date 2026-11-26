use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use trainer_core::{
    AobPattern, BytePatch, FreezeManager, FreezeTarget, MemoryValue, PointerWidth, ProcessHandle,
    ScanOptions, ValueType,
};

use crate::aa::{activate_auto_assembler, ActiveAutoAssembler};
use crate::error::{Result, SchemaError};
use crate::schema::{
    parse_hex_bytes, parse_hex_isize, parse_hex_usize, AddressSpec, EntryKind, ScalarValue,
    TrainerManifest,
};

/// Active runtime session binding a `TrainerManifest` to an attached `ProcessHandle`.
pub struct TrainerSession {
    process: Arc<ProcessHandle>,
    manifest: TrainerManifest,
    freezer: FreezeManager,
    patches: HashMap<String, BytePatch>,
    aa_scripts: HashMap<String, ActiveAutoAssembler>,
    symbols: HashMap<String, usize>,
}

impl TrainerSession {
    pub fn new(process: Arc<ProcessHandle>, manifest: TrainerManifest) -> Result<Self> {
        manifest.validate()?;
        let freezer = FreezeManager::start(Arc::clone(&process), Duration::from_millis(50));
        Ok(Self {
            process,
            manifest,
            freezer,
            patches: HashMap::new(),
            aa_scripts: HashMap::new(),
            symbols: HashMap::new(),
        })
    }

    pub fn manifest(&self) -> &TrainerManifest {
        &self.manifest
    }

    pub fn process(&self) -> &Arc<ProcessHandle> {
        &self.process
    }

    pub fn resolve_address(&self, spec: &AddressSpec, entry_id: &str) -> Result<usize> {
        let width: PointerWidth = self.manifest.game.pointer_width.into();
        match spec {
            AddressSpec::ModulePointerChain {
                module,
                base_offset,
                offsets,
            } => {
                let base_off = parse_hex_usize(base_offset)?;
                let parsed_offsets: Vec<isize> = offsets
                    .iter()
                    .map(|o| parse_hex_isize(o))
                    .collect::<Result<_>>()?;
                Ok(self
                    .process
                    .resolve_module_pointer_chain(module, base_off, &parsed_offsets, width)?)
            }
            AddressSpec::Absolute { address, offsets } => {
                let addr = parse_hex_usize(address)?;
                let parsed_offsets: Vec<isize> = offsets
                    .iter()
                    .map(|o| parse_hex_isize(o))
                    .collect::<Result<_>>()?;
                Ok(self
                    .process
                    .resolve_pointer_chain(addr, &parsed_offsets, width)?)
            }
            AddressSpec::AobOffset {
                module,
                pattern,
                offset,
                executable_only,
            } => {
                let pat = AobPattern::parse(pattern)?;
                let hits = if let Some(mod_name) = module {
                    self.process.scan_module_aob(mod_name, &pat, *executable_only)?
                } else {
                    self.process.scan_aob(
                        &pat,
                        ScanOptions {
                            executable_only: *executable_only,
                            max_results: Some(1),
                            ..Default::default()
                        },
                    )?
                };
                let first = hits
                    .into_iter()
                    .next()
                    .ok_or_else(|| SchemaError::PatternNotFound {
                        id: entry_id.to_string(),
                    })?;
                Ok(first.wrapping_add_signed(*offset))
            }
            AddressSpec::Symbol { symbol, offsets } => {
                let base = *self
                    .symbols
                    .get(&symbol.to_ascii_lowercase())
                    .ok_or_else(|| SchemaError::SymbolNotRegistered(symbol.clone()))?;
                let parsed_offsets: Vec<isize> = offsets
                    .iter()
                    .map(|o| parse_hex_isize(o))
                    .collect::<Result<_>>()?;
                Ok(self
                    .process
                    .resolve_pointer_chain(base, &parsed_offsets, width)?)
            }
        }
    }

    pub fn read_entry(&self, entry_id: &str) -> Result<MemoryValue> {
        let entry = self
            .manifest
            .find_entry(entry_id)
            .ok_or_else(|| SchemaError::EntryNotFound(entry_id.into()))?;
        match &entry.kind {
            EntryKind::Value {
                address,
                value_type,
                ..
            } => {
                let addr = self.resolve_address(address, entry_id)?;
                let ty: ValueType = (*value_type).into();
                Ok(self.process.read_value(addr, ty)?)
            }
            _ => Err(SchemaError::WrongEntryKind {
                id: entry_id.into(),
            }),
        }
    }

    pub fn write_entry(&self, entry_id: &str, value: &ScalarValue) -> Result<()> {
        let entry = self
            .manifest
            .find_entry(entry_id)
            .ok_or_else(|| SchemaError::EntryNotFound(entry_id.into()))?;
        match &entry.kind {
            EntryKind::Value {
                address,
                extra_addresses,
                value_type,
                ..
            } => {
                let mem_val = value.to_memory_value(*value_type);
                let addr = self.resolve_address(address, entry_id)?;
                self.process.write_value(addr, &mem_val)?;
                for extra in extra_addresses {
                    let extra_addr = self.resolve_address(extra, entry_id)?;
                    self.process.write_value(extra_addr, &mem_val)?;
                }
                Ok(())
            }
            _ => Err(SchemaError::WrongEntryKind {
                id: entry_id.into(),
            }),
        }
    }

    fn build_freeze_target(
        &self,
        spec: &AddressSpec,
        entry_id: &str,
        width: PointerWidth,
    ) -> Result<FreezeTarget> {
        match spec {
            AddressSpec::ModulePointerChain {
                module,
                base_offset,
                offsets,
            } => Ok(FreezeTarget::ModulePointerChain {
                module: module.clone(),
                base_offset: parse_hex_usize(base_offset)?,
                offsets: offsets
                    .iter()
                    .map(|o| parse_hex_isize(o))
                    .collect::<Result<_>>()?,
                width,
            }),
            AddressSpec::Absolute { address, offsets } => Ok(FreezeTarget::PointerChain {
                base_address: parse_hex_usize(address)?,
                offsets: offsets
                    .iter()
                    .map(|o| parse_hex_isize(o))
                    .collect::<Result<_>>()?,
                width,
            }),
            AddressSpec::AobOffset { .. } => {
                let resolved = self.resolve_address(spec, entry_id)?;
                Ok(FreezeTarget::Absolute(resolved))
            }
            AddressSpec::Symbol { symbol, offsets } => {
                let base = *self
                    .symbols
                    .get(&symbol.to_ascii_lowercase())
                    .ok_or_else(|| SchemaError::SymbolNotRegistered(symbol.clone()))?;
                Ok(FreezeTarget::PointerChain {
                    base_address: base,
                    offsets: offsets
                        .iter()
                        .map(|o| parse_hex_isize(o))
                        .collect::<Result<_>>()?,
                    width,
                })
            }
        }
    }

    pub fn set_freeze(&self, entry_id: &str, enabled: bool, value: Option<&ScalarValue>) -> Result<()> {
        let entry = self
            .manifest
            .find_entry(entry_id)
            .ok_or_else(|| SchemaError::EntryNotFound(entry_id.into()))?;

        if !enabled {
            self.freezer.unfreeze(entry_id);
            if let EntryKind::Value { extra_addresses, .. } = &entry.kind {
                for idx in 0..extra_addresses.len() {
                    self.freezer.unfreeze(&format!("{entry_id}#{idx}"));
                }
            }
            return Ok(());
        }

        let width: PointerWidth = self.manifest.game.pointer_width.into();

        match &entry.kind {
            EntryKind::Value {
                address,
                extra_addresses,
                value_type,
                default_value,
                ..
            } => {
                let mem_val = if let Some(v) = value {
                    v.to_memory_value(*value_type)
                } else if let Some(dv) = default_value {
                    dv.to_memory_value(*value_type)
                } else {
                    self.read_entry(entry_id)?
                };

                let target = self.build_freeze_target(address, entry_id, width)?;
                self.freezer.freeze(entry_id, target, mem_val.clone());
                for (idx, extra) in extra_addresses.iter().enumerate() {
                    let extra_target = self.build_freeze_target(extra, entry_id, width)?;
                    self.freezer
                        .freeze(format!("{entry_id}#{idx}"), extra_target, mem_val.clone());
                }
                Ok(())
            }
            _ => Err(SchemaError::WrongEntryKind {
                id: entry_id.into(),
            }),
        }
    }

    pub fn toggle_patch(&mut self, entry_id: &str, enabled: bool) -> Result<()> {
        let entry = self
            .manifest
            .find_entry(entry_id)
            .ok_or_else(|| SchemaError::EntryNotFound(entry_id.into()))?
            .clone();

        match entry.kind {
            EntryKind::AobPatch {
                module,
                pattern,
                patch_offset,
                patch_bytes,
                executable_only,
            } => {
                if enabled {
                    if self
                        .patches
                        .get(entry_id)
                        .is_some_and(|p| p.is_applied())
                    {
                        return Ok(());
                    }
                    let pat = AobPattern::parse(&pattern)?;
                    let hits = if let Some(mod_name) = &module {
                        self.process
                            .scan_module_aob(mod_name, &pat, executable_only)?
                    } else {
                        self.process.scan_aob(
                            &pat,
                            ScanOptions {
                                executable_only,
                                max_results: Some(1),
                                ..Default::default()
                            },
                        )?
                    };
                    let base = hits
                        .into_iter()
                        .next()
                        .ok_or_else(|| SchemaError::PatternNotFound {
                            id: entry_id.to_string(),
                        })?;
                    let addr = base.wrapping_add_signed(patch_offset);
                    let bytes = parse_hex_bytes(&patch_bytes)?;
                    let mut patch = BytePatch::new(addr, bytes);
                    patch.apply(&self.process)?;
                    self.patches.insert(entry_id.to_string(), patch);
                    Ok(())
                } else {
                    if let Some(patch) = self.patches.get_mut(entry_id) {
                        if patch.is_applied() {
                            patch.revert(&self.process)?;
                        }
                    }
                    Ok(())
                }
            }
            EntryKind::AutoAssembler { script } => {
                if enabled {
                    if self.aa_scripts.contains_key(entry_id) {
                        return Ok(());
                    }
                    let active = activate_auto_assembler(&self.process, &script)?;
                    for (sym, addr) in &active.registered_symbols {
                        self.symbols.insert(sym.to_ascii_lowercase(), *addr);
                    }
                    self.aa_scripts.insert(entry_id.to_string(), active);
                    Ok(())
                } else {
                    if let Some(mut active) = self.aa_scripts.remove(entry_id) {
                        for sym in active.registered_symbols.keys() {
                            self.symbols.remove(&sym.to_ascii_lowercase());
                        }
                        active.deactivate(&self.process);
                    }
                    Ok(())
                }
            }
            _ => Err(SchemaError::WrongEntryKind {
                id: entry_id.into(),
            }),
        }
    }

    pub fn is_frozen(&self, entry_id: &str) -> bool {
        self.freezer.is_frozen(entry_id)
    }

    pub fn is_patch_active(&self, entry_id: &str) -> bool {
        self.patches.get(entry_id).is_some_and(|p| p.is_applied())
            || self.aa_scripts.contains_key(entry_id)
    }

    /// Unfreezes all values, reverts all active byte patches, and deactivates all AA scripts.
    pub fn disable_all(&mut self) {
        self.freezer.unfreeze_all();
        for patch in self.patches.values_mut() {
            if patch.is_applied() {
                let _ = patch.revert(&self.process);
            }
        }
        for (_, mut active) in self.aa_scripts.drain() {
            active.deactivate(&self.process);
        }
        self.symbols.clear();
    }
}

impl Drop for TrainerSession {
    fn drop(&mut self) {
        self.disable_all();
    }
}
