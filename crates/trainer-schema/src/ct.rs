use std::collections::HashSet;
use std::io::Cursor;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};

use crate::error::{Result, SchemaError};
use crate::schema::{
    AddressSpec, EntryKind, GameTarget, PointerWidthSchema, TrainerEntry, TrainerManifest,
    ValueTypeSchema, CURRENT_SCHEMA_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub manifest: TrainerManifest,
    pub skipped: Vec<SkippedEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub id: String,
    pub game_title: String,
    pub process_name: String,
    pub author: String,
    pub pointer_width: PointerWidthSchema,
    /// When true, imports non-Lua x64 Auto Assembler scripts and symbol-based child addresses.
    pub allow_auto_assembler: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            id: "imported-table".into(),
            game_title: "Imported Game".into(),
            process_name: "game.exe".into(),
            author: "community".into(),
            pointer_width: PointerWidthSchema::Bits64,
            allow_auto_assembler: false,
        }
    }
}

#[derive(Debug, Default)]
struct RawCtEntry {
    description: String,
    variable_type: Option<String>,
    address: Option<String>,
    offsets: Vec<String>,
    assembler_script: Option<String>,
    is_group_header: bool,
    children: Vec<RawCtEntry>,
}

/// Imports declarative value & pointer entries from a Cheat Engine `.CT` XML string.
///
/// Auto Assembler scripts and Lua scripts are intentionally skipped and reported in
/// `ImportResult.skipped` to prevent arbitrary code execution from community tables.
pub fn import_ct_xml(xml: &str, opts: ImportOptions) -> Result<ImportResult> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut root_entries: Vec<RawCtEntry> = Vec::new();
    let mut stack: Vec<RawCtEntry> = Vec::new();
    let mut current_tag: Option<String> = None;
    let mut current_text = String::new();
    let mut in_offsets = false;
    let mut has_lua_script = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = e.name().as_ref().to_owned();
                match tag.as_str() {
                    "CheatEntry" => {
                        stack.push(RawCtEntry::default());
                    }
                    "Offsets" => {
                        in_offsets = true;
                    }
                    "LuaScript" => {
                        has_lua_script = true;
                    }
                    _ => {}
                }
                current_tag = Some(tag);
                current_text.clear();
            }
            Ok(Event::Text(e)) => {
                let text = e.xml_content(quick_xml::XmlVersion::default());
                current_text.push_str(&text);
            }
            Ok(Event::CData(e)) => {
                let text = e.xml_content(quick_xml::XmlVersion::default());
                current_text.push_str(&text);
            }
            Ok(Event::GeneralRef(e)) => {
                if let Some(ch) = e.resolve_char_ref().map_err(SchemaError::Xml)? {
                    current_text.push(ch);
                } else {
                    let name = e.xml_content(quick_xml::XmlVersion::default());
                    match quick_xml::escape::resolve_xml_entity(&name) {
                        Some(resolved) => current_text.push_str(resolved),
                        None => {
                            current_text.push('&');
                            current_text.push_str(&name);
                            current_text.push(';');
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag = e.name().as_ref().to_owned();
                if let (Some(entry), Some(active_tag)) = (stack.last_mut(), current_tag.as_deref()) {
                    if active_tag == tag && !current_text.is_empty() {
                        match active_tag {
                            "Description" => entry.description = strip_ct_quotes(&current_text),
                            "VariableType" => entry.variable_type = Some(current_text.clone()),
                            "Address" => entry.address = Some(current_text.clone()),
                            "Offset" if in_offsets => entry.offsets.push(current_text.clone()),
                            "AssemblerScript" => {
                                entry
                                    .assembler_script
                                    .get_or_insert_with(String::new)
                                    .push_str(&current_text);
                            }
                            "GroupHeader" if current_text == "1" => entry.is_group_header = true,
                            _ => {}
                        }
                    }
                }
                match tag.as_str() {
                    "CheatEntry" => {
                        if let Some(finished) = stack.pop() {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(finished);
                            } else {
                                root_entries.push(finished);
                            }
                        }
                    }
                    "Offsets" => {
                        in_offsets = false;
                    }
                    _ => {}
                }
                current_tag = None;
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(SchemaError::Xml(e)),
            _ => {}
        }
    }

    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    let mut used_ids = HashSet::new();

    if has_lua_script {
        warnings.push(
            "Table contains a <LuaScript> block which was ignored for security (declarative entries only)."
                .into(),
        );
    }

    flatten_raw_entries(
        &root_entries,
        None,
        opts.allow_auto_assembler,
        &mut entries,
        &mut skipped,
        &mut used_ids,
    );

    let manifest = TrainerManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: opts.id,
        game: GameTarget {
            title: opts.game_title,
            processes: vec![opts.process_name],
            steam_app_id: None,
            game_version: None,
            pointer_width: opts.pointer_width,
        },
        author: opts.author,
        version: "1.0.0".into(),
        description: Some("Imported from Cheat Engine .CT table".into()),
        entries,
    };

    manifest.validate()?;

    Ok(ImportResult {
        manifest,
        skipped,
        warnings,
    })
}

fn flatten_raw_entries(
    raw_list: &[RawCtEntry],
    category: Option<&str>,
    allow_aa: bool,
    out: &mut Vec<TrainerEntry>,
    skipped: &mut Vec<SkippedEntry>,
    used_ids: &mut HashSet<String>,
) {
    for raw in raw_list {
        let label = if raw.description.trim().is_empty() {
            "Unnamed Entry".to_string()
        } else {
            raw.description.trim().to_string()
        };

        // Pure group header -> pass label as category to children
        if raw.is_group_header && raw.address.is_none() {
            flatten_raw_entries(&raw.children, Some(&label), allow_aa, out, skipped, used_ids);
            continue;
        }

        let var_type_raw = raw.variable_type.as_deref().unwrap_or("");
        if var_type_raw.eq_ignore_ascii_case("Auto Assembler Script") {
            if let Some(script) = raw.assembler_script.as_deref() {
                if let Some(aob_kind) = extract_declarative_aob_patch(script) {
                    let id = unique_slug(&label, used_ids);
                    out.push(TrainerEntry {
                        id,
                        label: label.clone(),
                        category: category.map(ToString::to_string),
                        description: Some("Extracted declarative AoB byte patch from .CT".into()),
                        hotkey: None,
                        kind: aob_kind,
                    });
                    flatten_raw_entries(&raw.children, Some(&label), allow_aa, out, skipped, used_ids);
                    continue;
                }

                let lc = script.to_ascii_lowercase();
                let has_lua = lc.contains("{$lua}") || lc.contains("luacall(");
                if allow_aa && !has_lua && lc.contains("[enable]") {
                    let id = unique_slug(&label, used_ids);
                    out.push(TrainerEntry {
                        id,
                        label: label.clone(),
                        category: category.map(ToString::to_string),
                        description: Some("x64 Auto Assembler script (opt-in)".into()),
                        hotkey: None,
                        kind: EntryKind::AutoAssembler {
                            script: script.to_string(),
                        },
                    });
                    flatten_raw_entries(&raw.children, Some(&label), allow_aa, out, skipped, used_ids);
                    continue;
                }
            }
            skipped.push(SkippedEntry {
                label: label.clone(),
                reason: if allow_aa {
                    "Contains Lua script (Lua is disabled for security)".into()
                } else {
                    "Auto Assembler script uses code injection or Lua (enable opt-in AA scripts to import non-Lua code caves)".into()
                },
            });
            flatten_raw_entries(&raw.children, category, allow_aa, out, skipped, used_ids);
            continue;
        }

        let Some(value_type) = map_ct_variable_type(var_type_raw) else {
            if !raw.children.is_empty() && raw.address.is_none() {
                flatten_raw_entries(&raw.children, Some(&label), allow_aa, out, skipped, used_ids);
            } else {
                skipped.push(SkippedEntry {
                    label: label.clone(),
                    reason: format!("unsupported VariableType '{var_type_raw}'"),
                });
            }
            continue;
        };

        let Some(addr_raw) = raw.address.as_deref() else {
            skipped.push(SkippedEntry {
                label: label.clone(),
                reason: "missing <Address>".into(),
            });
            continue;
        };

        // Cheat Engine stores <Offsets> in reverse order (last offset is applied first).
        let offsets: Vec<String> = raw
            .offsets
            .iter()
            .rev()
            .map(|o| normalize_hex_offset(o))
            .collect();

        let address = match parse_ct_address(addr_raw, offsets, allow_aa) {
            Ok(spec) => spec,
            Err(reason) => {
                skipped.push(SkippedEntry {
                    label: label.clone(),
                    reason,
                });
                continue;
            }
        };

        let id = unique_slug(&label, used_ids);
        out.push(TrainerEntry {
            id,
            label: label.clone(),
            category: category.map(ToString::to_string),
            description: None,
            hotkey: None,
            kind: EntryKind::Value {
                address,
                extra_addresses: Vec::new(),
                value_type,
                default_value: None,
                freeze: false,
                min: None,
                max: None,
                step: None,
            },
        });

        // Also import any nested child entries under this entry's label as category
        if !raw.children.is_empty() {
            flatten_raw_entries(&raw.children, Some(&label), allow_aa, out, skipped, used_ids);
        }
    }
}

/// Exports declarative value entries from a `TrainerManifest` into Cheat Engine `.CT` XML.
/// Returns `(xml_string, skipped_entries)` where `skipped_entries` lists any entries
/// (such as `AobPatch` or `AobOffset`) that have no direct declarative `.CT` value representation.
pub fn export_ct_xml(manifest: &TrainerManifest) -> Result<(String, Vec<SkippedEntry>)> {
    manifest.validate()?;

    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    let mut root = BytesStart::new("CheatTable");
    root.push_attribute(("CheatEngineTableVersion", "45"));
    writer.write_event(Event::Start(root))?;

    writer.write_event(Event::Start(BytesStart::new("CheatEntries")))?;

    let mut skipped = Vec::new();
    let mut next_id: u32 = 1;

    for entry in &manifest.entries {
        match &entry.kind {
            EntryKind::Value {
                address,
                value_type,
                ..
            } => {
                let (addr_str, offsets) = match address {
                    AddressSpec::ModulePointerChain {
                        module,
                        base_offset,
                        offsets,
                    } => {
                        let clean_off = strip_0x(base_offset).to_ascii_uppercase();
                        (format!("\"{module}\"+{clean_off}"), offsets.as_slice())
                    }
                    AddressSpec::Absolute { address, offsets } => {
                        (strip_0x(address).to_ascii_uppercase(), offsets.as_slice())
                    }
                    AddressSpec::AobOffset { .. } => {
                        skipped.push(SkippedEntry {
                            label: entry.label.clone(),
                            reason: "AobOffset entries cannot be exported as static .CT addresses"
                                .into(),
                        });
                        continue;
                    }
                    AddressSpec::Symbol { symbol, offsets } => (symbol.clone(), offsets.as_slice()),
                };

                writer.write_event(Event::Start(BytesStart::new("CheatEntry")))?;
                write_text_elem(&mut writer, "ID", &next_id.to_string())?;
                next_id += 1;
                write_text_elem(&mut writer, "Description", &format!("\"{}\"", entry.label))?;
                write_text_elem(&mut writer, "VariableType", to_ct_variable_type(*value_type))?;
                write_text_elem(&mut writer, "Address", &addr_str)?;

                if !offsets.is_empty() {
                    writer.write_event(Event::Start(BytesStart::new("Offsets")))?;
                    // Reverse back to Cheat Engine's bottom-to-top XML order.
                    for off in offsets.iter().rev() {
                        write_text_elem(&mut writer, "Offset", &strip_0x(off).to_ascii_uppercase())?;
                    }
                    writer.write_event(Event::End(BytesEnd::new("Offsets")))?;
                }

                writer.write_event(Event::End(BytesEnd::new("CheatEntry")))?;
            }
            EntryKind::AobPatch { .. } => {
                skipped.push(SkippedEntry {
                    label: entry.label.clone(),
                    reason: "AobPatch entries are not exported to .CT (use native JSON format)"
                        .into(),
                });
            }
            EntryKind::AutoAssembler { script } => {
                writer.write_event(Event::Start(BytesStart::new("CheatEntry")))?;
                write_text_elem(&mut writer, "ID", &next_id.to_string())?;
                next_id += 1;
                write_text_elem(&mut writer, "Description", &format!("\"{}\"", entry.label))?;
                write_text_elem(&mut writer, "VariableType", "Auto Assembler Script")?;
                write_text_elem(&mut writer, "AssemblerScript", script)?;
                writer.write_event(Event::End(BytesEnd::new("CheatEntry")))?;
            }
        }
    }

    writer.write_event(Event::End(BytesEnd::new("CheatEntries")))?;
    writer.write_event(Event::End(BytesEnd::new("CheatTable")))?;

    let bytes = writer.into_inner().into_inner();
    let xml = String::from_utf8(bytes)
        .map_err(|e| SchemaError::Validation(format!("UTF-8 encoding error: {e}")))?;

    Ok((xml, skipped))
}

fn write_text_elem(writer: &mut Writer<Cursor<Vec<u8>>>, tag: &str, text: &str) -> Result<()> {
    writer.write_event(Event::Start(BytesStart::new(tag)))?;
    writer.write_event(Event::Text(BytesText::new(text)))?;
    writer.write_event(Event::End(BytesEnd::new(tag)))?;
    Ok(())
}

fn strip_ct_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_0x(s: &str) -> String {
    let t = s.trim();
    if let Some(neg) = t.strip_prefix('-') {
        let rest = neg
            .strip_prefix("0x")
            .or_else(|| neg.strip_prefix("0X"))
            .unwrap_or(neg);
        format!("-{rest}")
    } else {
        t.strip_prefix("0x")
            .or_else(|| t.strip_prefix("0X"))
            .unwrap_or(t)
            .to_string()
    }
}

fn normalize_hex_offset(raw: &str) -> String {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix('-') {
        format!("-0x{}", strip_0x(rest).to_ascii_uppercase())
    } else {
        format!("0x{}", strip_0x(t).to_ascii_uppercase())
    }
}

fn parse_ct_address(
    raw: &str,
    offsets: Vec<String>,
    allow_symbols: bool,
) -> std::result::Result<AddressSpec, String> {
    let trimmed = raw.trim();
    if let Some((mod_part, off_part)) = trimmed.split_once('+') {
        let module = strip_ct_quotes(mod_part);
        let base_offset = format!("0x{}", strip_0x(off_part).to_ascii_uppercase());
        if module.is_empty() {
            return Err(format!("invalid module address '{trimmed}'"));
        }
        Ok(AddressSpec::ModulePointerChain {
            module,
            base_offset,
            offsets,
        })
    } else {
        let hex = strip_0x(trimmed);
        if usize::from_str_radix(&hex, 16).is_ok() {
            Ok(AddressSpec::Absolute {
                address: format!("0x{}", hex.to_ascii_uppercase()),
                offsets,
            })
        } else if allow_symbols
            && !trimmed.is_empty()
            && trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            Ok(AddressSpec::Symbol {
                symbol: trimmed.to_string(),
                offsets,
            })
        } else {
            Err(format!(
                "symbolic or non-hex address '{trimmed}' is not supported"
            ))
        }
    }
}

fn map_ct_variable_type(raw: &str) -> Option<ValueTypeSchema> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "byte" => Some(ValueTypeSchema::U8),
        "2 bytes" => Some(ValueTypeSchema::I16),
        "4 bytes" => Some(ValueTypeSchema::I32),
        "8 bytes" => Some(ValueTypeSchema::I64),
        "float" => Some(ValueTypeSchema::F32),
        "double" => Some(ValueTypeSchema::F64),
        _ => None,
    }
}

fn to_ct_variable_type(ty: ValueTypeSchema) -> &'static str {
    match ty {
        ValueTypeSchema::U8 => "Byte",
        ValueTypeSchema::I16 | ValueTypeSchema::U16 => "2 Bytes",
        ValueTypeSchema::I32 | ValueTypeSchema::U32 => "4 Bytes",
        ValueTypeSchema::I64 | ValueTypeSchema::U64 => "8 Bytes",
        ValueTypeSchema::F32 => "Float",
        ValueTypeSchema::F64 => "Double",
    }
}

fn unique_slug(label: &str, used: &mut HashSet<String>) -> String {
    let base: String = label
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = base.trim_matches('_');
    let stem = if trimmed.is_empty() { "entry" } else { trimmed };

    let mut candidate = stem.to_string();
    let mut n = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{stem}_{n}");
        n += 1;
    }
    candidate
}

/// Safely extracts a pure declarative `EntryKind::AobPatch` from a Cheat Engine
/// Auto Assembler script if (and only if) the `[ENABLE]` block performs a simple
/// `aobscanmodule` / `aobscan` + `db <hex bytes>` patch with no Lua or code-cave injection.
fn extract_declarative_aob_patch(script: &str) -> Option<EntryKind> {
    let lower_full = script.to_ascii_lowercase();
    if lower_full.contains("{$lua}") || lower_full.contains("luacall(") {
        return None;
    }

    let stripped = strip_curly_comments(script);
    let enable_body = extract_enable_section(&stripped)?;

    let mut aob_sym: Option<String> = None;
    let mut aob_module: Option<String> = None;
    let mut aob_pattern: Option<String> = None;

    let mut current_label: Option<String> = None;
    let mut patch_offset: Option<isize> = None;
    let mut patch_bytes: Option<String> = None;

    for raw_line in enable_body.lines() {
        let line = raw_line
            .split("//")
            .next()
            .unwrap_or("")
            .trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();

        // Reject any code-cave assembly instructions
        if lower.starts_with("jmp ")
            || lower.starts_with("call ")
            || lower.starts_with("mov ")
            || lower.starts_with("movss ")
            || lower.starts_with("mulss ")
            || lower.starts_with("addss ")
            || lower.starts_with("subss ")
            || lower.starts_with("push ")
            || lower.starts_with("pop ")
            || lower == "ret"
            || lower.starts_with("newmem:")
        {
            return None;
        }

        if let Some(args) = strip_call_prefix(line, "aobscanmodule") {
            let parts: Vec<&str> = args.splitn(3, ',').map(str::trim).collect();
            if parts.len() == 3 {
                aob_sym = Some(parts[0].to_string());
                aob_module = Some(strip_ct_quotes(parts[1]));
                aob_pattern = Some(parts[2].to_string());
            }
            continue;
        }

        if let Some(args) = strip_call_prefix(line, "aobscan") {
            let parts: Vec<&str> = args.splitn(2, ',').map(str::trim).collect();
            if parts.len() == 2 {
                aob_sym = Some(parts[0].to_string());
                aob_module = None;
                aob_pattern = Some(parts[1].to_string());
            }
            continue;
        }

        // Ignore harmless CE bookkeeping directives (our BytePatch saves original bytes automatically)
        if lower.starts_with("registersymbol(")
            || lower.starts_with("unregistersymbol(")
            || lower.starts_with("alloc(")
            || lower.starts_with("dealloc(")
            || lower.starts_with("label(")
            || lower.starts_with("readmem(")
        {
            continue;
        }

        if let Some(lbl) = line.strip_suffix(':') {
            current_label = Some(lbl.trim().to_string());
            continue;
        }

        if let Some(rest) = line.strip_prefix("db ").or_else(|| line.strip_prefix("DB ")) {
            let sym = aob_sym.as_deref()?;
            let lbl = current_label.as_deref()?;
            let off = parse_label_offset(lbl, sym)?;
            let bytes = rest.trim().to_string();
            // Reject multiple patches in a single script for now
            if patch_bytes.is_some() {
                return None;
            }
            patch_offset = Some(off);
            patch_bytes = Some(bytes);
            continue;
        }

        // Unknown instruction -> bail out safely
        return None;
    }

    let pattern = aob_pattern?;
    let bytes = patch_bytes?;
    let offset = patch_offset?;

    // Validate pattern and patch bytes before accepting
    trainer_core::AobPattern::parse(&pattern).ok()?;
    crate::schema::parse_hex_bytes(&bytes).ok()?;

    Some(EntryKind::AobPatch {
        module: aob_module,
        pattern,
        patch_offset: offset,
        patch_bytes: bytes,
        executable_only: true,
    })
}

fn strip_curly_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn extract_enable_section(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let start_idx = lower.find("[enable]")? + "[enable]".len();
    let rest = &script[start_idx..];
    let rest_lower = &lower[start_idx..];
    if let Some(end_idx) = rest_lower.find("[disable]") {
        Some(&rest[..end_idx])
    } else {
        Some(rest)
    }
}

fn strip_call_prefix<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let lower = line.to_ascii_lowercase();
    let prefix = format!("{name}(");
    if !lower.starts_with(&prefix) {
        return None;
    }
    let inner = &line[prefix.len()..];
    let end = inner.rfind(')')?;
    Some(inner[..end].trim())
}

fn parse_label_offset(label: &str, sym: &str) -> Option<isize> {
    if label.eq_ignore_ascii_case(sym) {
        return Some(0);
    }
    let lower_label = label.to_ascii_lowercase();
    let lower_sym = sym.to_ascii_lowercase();
    if let Some(rest) = lower_label.strip_prefix(&lower_sym) {
        let rest = rest.trim();
        if let Some(hex) = rest.strip_prefix('+') {
            return crate::schema::parse_hex_isize(hex).ok();
        }
        if let Some(hex) = rest.strip_prefix('-') {
            return crate::schema::parse_hex_isize(&format!("-{hex}")).ok();
        }
    }
    None
}
