#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State, Window};
use trainer_core::{
    find_processes_by_name, list_processes, resolve_static_module, MemoryScanner, MemoryValue,
    ProcessHandle, ScanFilter, ValueType,
};
use trainer_repo::{
    compute_sha256_hex, AvailableUpdate, CatalogHit, InstalledRecord, RepoIndex, RepoManager,
    RepoSource, RepoTrainerItem,
};
use trainer_schema::{
    export_ct_xml, import_ct_xml, AddressSpec, EntryKind, GameTarget, ImportOptions,
    PointerWidthSchema, ScalarValue, TrainerEntry, TrainerManifest, TrainerSession,
    ValueTypeSchema, CURRENT_SCHEMA_VERSION,
};

struct ActiveSession {
    trainer_id: String,
    process_name: String,
    session: TrainerSession,
}

struct ActiveScanner {
    process: ProcessHandle,
    scanner: MemoryScanner,
}

struct AppState {
    repo: RepoManager,
    active: Mutex<Option<ActiveSession>>,
    scanner: Arc<Mutex<Option<ActiveScanner>>>,
}

#[derive(Debug, Serialize)]
struct SessionStatus {
    attached: bool,
    trainer_id: Option<String>,
    process_name: Option<String>,
    pid: Option<u32>,
    alive: bool,
}

#[derive(Debug, Serialize)]
struct RefreshSummary {
    refreshed: usize,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CtImportSummary {
    record: InstalledRecord,
    imported_entries: usize,
    skipped: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CtExportSummary {
    xml: String,
    skipped: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProcessDto {
    pid: u32,
    name: String,
    window_title: Option<String>,
    exe_path: Option<String>,
    is_system: bool,
}

#[derive(Debug, Serialize)]
struct ScanHitDto {
    address: String,
    value_type: ValueTypeSchema,
    value: String,
    previous: Option<String>,
    static_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScanResponse {
    total_hits: usize,
    preview: Vec<ScanHitDto>,
}

#[derive(Debug, Clone, Serialize)]
struct ScanProgressDto {
    phase: &'static str,
    current: usize,
    total: usize,
    regions_done: usize,
    regions_total: usize,
    hits_found: usize,
    percent: f32,
}

#[derive(Debug, Deserialize)]
struct CapturedEntryInput {
    label: String,
    category: Option<String>,
    address: String,
    value_type: ValueTypeSchema,
    default_value: Option<ScalarValue>,
    freeze: bool,
}

#[derive(Debug, Serialize)]
struct ScannerExportResult {
    trainer_id: String,
    trainer_json: String,
    sha256: String,
    ct_xml: String,
    repo_index_entry: String,
}

fn format_memory_value(val: &MemoryValue) -> String {
    match val {
        MemoryValue::U8(v) => v.to_string(),
        MemoryValue::I16(v) => v.to_string(),
        MemoryValue::U16(v) => v.to_string(),
        MemoryValue::I32(v) => v.to_string(),
        MemoryValue::U32(v) => v.to_string(),
        MemoryValue::I64(v) => v.to_string(),
        MemoryValue::U64(v) => v.to_string(),
        MemoryValue::F32(v) => format!("{v:.3}"),
        MemoryValue::F64(v) => format!("{v:.4}"),
        MemoryValue::Bytes(b) => b
            .iter()
            .map(|x| format!("{x:02X}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[tauri::command]
fn list_repos(state: State<'_, AppState>) -> Result<Vec<RepoSource>, String> {
    state.repo.list_repos().map_err(|e| e.to_string())
}

#[tauri::command]
fn add_repo(state: State<'_, AppState>, index_url: String) -> Result<RepoIndex, String> {
    state.repo.add_repo(index_url.trim()).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_repo(state: State<'_, AppState>, repo_id: String) -> Result<(), String> {
    state.repo.remove_repo(&repo_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn refresh_repos(state: State<'_, AppState>) -> Result<RefreshSummary, String> {
    let (ok, errs) = state.repo.refresh_all().map_err(|e| e.to_string())?;
    Ok(RefreshSummary {
        refreshed: ok.len(),
        errors: errs
            .into_iter()
            .map(|(id, e)| format!("{id}: {e}"))
            .collect(),
    })
}

#[tauri::command]
fn search_catalog(state: State<'_, AppState>, query: String) -> Result<Vec<CatalogHit>, String> {
    state.repo.search_catalog(&query).map_err(|e| e.to_string())
}

#[tauri::command]
fn check_updates(state: State<'_, AppState>) -> Result<Vec<AvailableUpdate>, String> {
    state.repo.check_updates().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_installed(state: State<'_, AppState>) -> Result<Vec<InstalledRecord>, String> {
    state.repo.list_installed().map_err(|e| e.to_string())
}

#[tauri::command]
fn load_trainer(state: State<'_, AppState>, trainer_id: String) -> Result<TrainerManifest, String> {
    state
        .repo
        .load_installed(&trainer_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn install_trainer(
    state: State<'_, AppState>,
    repo_id: String,
    trainer_id: String,
) -> Result<InstalledRecord, String> {
    state
        .repo
        .install_trainer(&repo_id, &trainer_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn uninstall_trainer(state: State<'_, AppState>, trainer_id: String) -> Result<(), String> {
    {
        let mut guard = state.active.lock().unwrap();
        if guard.as_ref().is_some_and(|a| a.trainer_id == trainer_id) {
            *guard = None;
        }
    }
    state
        .repo
        .uninstall_trainer(&trainer_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn import_ct(
    state: State<'_, AppState>,
    xml: String,
    id: String,
    game_title: String,
    process_name: String,
    author: String,
    pointer_width: String,
    allow_auto_assembler: bool,
) -> Result<CtImportSummary, String> {
    let pw = if pointer_width == "32" {
        PointerWidthSchema::Bits32
    } else {
        PointerWidthSchema::Bits64
    };
    let res = import_ct_xml(
        &xml,
        ImportOptions {
            id,
            game_title,
            process_name,
            author,
            pointer_width: pw,
            allow_auto_assembler,
        },
    )
    .map_err(|e| e.to_string())?;

    let count = res.manifest.entries.len();
    let record = state
        .repo
        .import_local_manifest(&res.manifest)
        .map_err(|e| e.to_string())?;

    Ok(CtImportSummary {
        record,
        imported_entries: count,
        skipped: res
            .skipped
            .into_iter()
            .map(|s| format!("{}: {}", s.label, s.reason))
            .collect(),
        warnings: res.warnings,
    })
}

#[tauri::command]
fn export_ct(state: State<'_, AppState>, trainer_id: String) -> Result<CtExportSummary, String> {
    let manifest = state
        .repo
        .load_installed(&trainer_id)
        .map_err(|e| e.to_string())?;
    let (xml, skipped) = export_ct_xml(&manifest).map_err(|e| e.to_string())?;
    Ok(CtExportSummary {
        xml,
        skipped: skipped
            .into_iter()
            .map(|s| format!("{}: {}", s.label, s.reason))
            .collect(),
    })
}

#[tauri::command]
fn import_json_manifest(
    state: State<'_, AppState>,
    json_text: String,
) -> Result<InstalledRecord, String> {
    let manifest = TrainerManifest::from_json_str(&json_text).map_err(|e| e.to_string())?;
    state
        .repo
        .import_local_manifest(&manifest)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_running_processes(include_system: Option<bool>) -> Result<Vec<ProcessDto>, String> {
    let show_sys = include_system.unwrap_or(false);
    let mut procs: Vec<ProcessDto> = list_processes()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|p| show_sys || !p.is_system)
        .map(|p| ProcessDto {
            pid: p.pid,
            name: p.name,
            window_title: p.window_title,
            exe_path: p.exe_path,
            is_system: p.is_system,
        })
        .collect();
    // Put processes with visible windows (games & desktop apps) first, then sort alphabetically.
    procs.sort_by(|a, b| {
        b.window_title
            .is_some()
            .cmp(&a.window_title.is_some())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(procs)
}

#[tauri::command]
fn attach_trainer_to_pid(
    state: State<'_, AppState>,
    trainer_id: String,
    pid: u32,
    process_name: String,
) -> Result<SessionStatus, String> {
    let manifest = state
        .repo
        .load_installed(&trainer_id)
        .map_err(|e| e.to_string())?;
    let handle = Arc::new(ProcessHandle::attach_pid(pid).map_err(|e| e.to_string())?);
    let session = TrainerSession::new(handle, manifest).map_err(|e| e.to_string())?;
    let status = SessionStatus {
        attached: true,
        trainer_id: Some(trainer_id.clone()),
        process_name: Some(process_name.clone()),
        pid: Some(pid),
        alive: true,
    };
    let mut guard = state.active.lock().unwrap();
    *guard = Some(ActiveSession {
        trainer_id,
        process_name,
        session,
    });
    Ok(status)
}

#[tauri::command]
fn attach_trainer(state: State<'_, AppState>, trainer_id: String) -> Result<SessionStatus, String> {
    let manifest = state
        .repo
        .load_installed(&trainer_id)
        .map_err(|e| e.to_string())?;

    let mut matched_proc = None;
    for exe in &manifest.game.processes {
        if let Ok(found) = find_processes_by_name(exe) {
            if let Some(p) = found.into_iter().next() {
                matched_proc = Some(p);
                break;
            }
        }
    }

    let proc_info = matched_proc.ok_or_else(|| {
        format!(
            "None of the target processes ({}) are currently running.",
            manifest.game.processes.join(", ")
        )
    })?;

    let handle = Arc::new(ProcessHandle::attach_pid(proc_info.pid).map_err(|e| e.to_string())?);
    let session = TrainerSession::new(handle, manifest).map_err(|e| e.to_string())?;

    let status = SessionStatus {
        attached: true,
        trainer_id: Some(trainer_id.clone()),
        process_name: Some(proc_info.name.clone()),
        pid: Some(proc_info.pid),
        alive: true,
    };

    let mut guard = state.active.lock().unwrap();
    *guard = Some(ActiveSession {
        trainer_id,
        process_name: proc_info.name,
        session,
    });
    Ok(status)
}

#[tauri::command]
fn detach_trainer(state: State<'_, AppState>) -> Result<SessionStatus, String> {
    let mut guard = state.active.lock().unwrap();
    if let Some(mut active) = guard.take() {
        active.session.disable_all();
    }
    Ok(SessionStatus {
        attached: false,
        trainer_id: None,
        process_name: None,
        pid: None,
        alive: false,
    })
}

#[tauri::command]
fn session_status(state: State<'_, AppState>) -> Result<SessionStatus, String> {
    let guard = state.active.lock().unwrap();
    if let Some(active) = guard.as_ref() {
        let alive = active.session.process().is_alive();
        Ok(SessionStatus {
            attached: true,
            trainer_id: Some(active.trainer_id.clone()),
            process_name: Some(active.process_name.clone()),
            pid: Some(active.session.process().pid()),
            alive,
        })
    } else {
        Ok(SessionStatus {
            attached: false,
            trainer_id: None,
            process_name: None,
            pid: None,
            alive: false,
        })
    }
}

#[tauri::command]
fn read_entry(state: State<'_, AppState>, entry_id: String) -> Result<String, String> {
    let guard = state.active.lock().unwrap();
    let active = guard.as_ref().ok_or("Not attached to any game process.")?;
    let val = active
        .session
        .read_entry(&entry_id)
        .map_err(|e| e.to_string())?;
    Ok(format_memory_value(&val))
}

#[tauri::command]
fn write_entry(
    state: State<'_, AppState>,
    entry_id: String,
    value: ScalarValue,
) -> Result<(), String> {
    let guard = state.active.lock().unwrap();
    let active = guard.as_ref().ok_or("Not attached to any game process.")?;
    active
        .session
        .write_entry(&entry_id, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_freeze(
    state: State<'_, AppState>,
    entry_id: String,
    enabled: bool,
    value: Option<ScalarValue>,
) -> Result<bool, String> {
    let guard = state.active.lock().unwrap();
    let active = guard.as_ref().ok_or("Not attached to any game process.")?;
    active
        .session
        .set_freeze(&entry_id, enabled, value.as_ref())
        .map_err(|e| e.to_string())?;
    Ok(active.session.is_frozen(&entry_id))
}

#[tauri::command]
fn toggle_patch(
    state: State<'_, AppState>,
    entry_id: String,
    enabled: bool,
) -> Result<bool, String> {
    let mut guard = state.active.lock().unwrap();
    let active = guard.as_mut().ok_or("Not attached to any game process.")?;
    active
        .session
        .toggle_patch(&entry_id, enabled)
        .map_err(|e| e.to_string())?;
    Ok(active.session.is_patch_active(&entry_id))
}

fn value_type_to_schema(ty: ValueType) -> ValueTypeSchema {
    match ty {
        ValueType::U8 => ValueTypeSchema::U8,
        ValueType::I16 => ValueTypeSchema::I16,
        ValueType::U16 => ValueTypeSchema::U16,
        ValueType::I32 => ValueTypeSchema::I32,
        ValueType::U32 => ValueTypeSchema::U32,
        ValueType::I64 => ValueTypeSchema::I64,
        ValueType::U64 => ValueTypeSchema::U64,
        ValueType::F32 => ValueTypeSchema::F32,
        ValueType::F64 => ValueTypeSchema::F64,
    }
}

fn parse_scan_value_type(raw: &str) -> Result<Option<ValueTypeSchema>, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(None),
        "u8" => Ok(Some(ValueTypeSchema::U8)),
        "i16" => Ok(Some(ValueTypeSchema::I16)),
        "u16" => Ok(Some(ValueTypeSchema::U16)),
        "i32" => Ok(Some(ValueTypeSchema::I32)),
        "u32" => Ok(Some(ValueTypeSchema::U32)),
        "i64" => Ok(Some(ValueTypeSchema::I64)),
        "u64" => Ok(Some(ValueTypeSchema::U64)),
        "f32" => Ok(Some(ValueTypeSchema::F32)),
        "f64" => Ok(Some(ValueTypeSchema::F64)),
        other => Err(format!("Unknown value_type '{other}'")),
    }
}

fn scalar_to_f64(v: &ScalarValue) -> f64 {
    match v {
        ScalarValue::Int(i) => *i as f64,
        ScalarValue::Float(f) => *f,
    }
}

fn build_scan_preview(active: &ActiveScanner, limit: usize) -> ScanResponse {
    let preview = active
        .scanner
        .hits()
        .iter()
        .take(limit)
        .map(|h| {
            let static_ref = resolve_static_module(&active.process, h.address)
                .map(|(m, off)| format!("{m}+0x{off:X}"));
            ScanHitDto {
                address: format!("0x{:X}", h.address),
                value_type: value_type_to_schema(h.value_type),
                value: format_memory_value(&h.value),
                previous: h.previous.as_ref().map(format_memory_value),
                static_ref,
            }
        })
        .collect();
    ScanResponse {
        total_hits: active.scanner.total_hits(),
        preview,
    }
}

#[tauri::command]
fn scanner_attach(
    state: State<'_, AppState>,
    pid: u32,
    process_name: String,
) -> Result<String, String> {
    let process = ProcessHandle::attach_pid(pid).map_err(|e| e.to_string())?;
    let mut guard = state.scanner.lock().unwrap();
    *guard = Some(ActiveScanner {
        process,
        scanner: MemoryScanner::new(ValueType::I32),
    });
    Ok(format!("{process_name} (PID {pid})"))
}

#[tauri::command]
fn scanner_detach(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.scanner.lock().unwrap();
    *guard = None;
    Ok(())
}

#[tauri::command]
async fn scanner_first_scan(
    window: Window,
    state: State<'_, AppState>,
    value_type: String,
    value: ScalarValue,
) -> Result<ScanResponse, String> {
    let parsed_ty = parse_scan_value_type(&value_type)?;
    let scanner_arc = Arc::clone(&state.scanner);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = scanner_arc.lock().unwrap();
        let active = guard.as_mut().ok_or("Pick a running process first.")?;

        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut emit_progress = |p: trainer_core::ScanProgress| {
            let finished = p.total > 0 && p.current >= p.total;
            if finished || last_emit.elapsed() >= Duration::from_millis(60) {
                last_emit = Instant::now();
                let percent = if p.total > 0 {
                    ((p.current as f64 / p.total as f64) * 100.0).min(100.0) as f32
                } else {
                    100.0
                };
                let _ = window.emit(
                    "scanner-progress",
                    ScanProgressDto {
                        phase: "first",
                        current: p.current,
                        total: p.total,
                        regions_done: p.regions_done,
                        regions_total: p.regions_total,
                        hits_found: p.hits_found,
                        percent,
                    },
                );
            }
        };

        if let Some(vt) = parsed_ty {
            let ty: ValueType = vt.into();
            let target = value.to_memory_value(vt);
            active.scanner = MemoryScanner::new(ty);
            active
                .scanner
                .first_scan_with_progress(&active.process, &target, emit_progress)
                .map_err(|e| e.to_string())?;
        } else {
            active.scanner = MemoryScanner::new(ValueType::I32);
            active
                .scanner
                .first_scan_all_with_progress(&active.process, scalar_to_f64(&value), emit_progress)
                .map_err(|e| e.to_string())?;
        }
        Ok(build_scan_preview(active, 150))
    })
    .await
    .map_err(|e| format!("scan task failed: {e}"))?
}

#[tauri::command]
async fn scanner_next_scan(
    window: Window,
    state: State<'_, AppState>,
    mode: String,
    value_type: String,
    value: Option<ScalarValue>,
) -> Result<ScanResponse, String> {
    let parsed_ty = parse_scan_value_type(&value_type)?;
    let scanner_arc = Arc::clone(&state.scanner);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = scanner_arc.lock().unwrap();
        let active = guard.as_mut().ok_or("Pick a running process first.")?;
        let filter = match mode.as_str() {
            "exact" => {
                let v = value.ok_or("Enter a target value for Exact scan.")?;
                if let Some(vt) = parsed_ty {
                    ScanFilter::Exact(v.to_memory_value(vt))
                } else {
                    ScanFilter::ExactNumber(scalar_to_f64(&v))
                }
            }
            "increased" => ScanFilter::Increased,
            "decreased" => ScanFilter::Decreased,
            "changed" => ScanFilter::Changed,
            "unchanged" => ScanFilter::Unchanged,
            other => return Err(format!("Unknown scan mode '{other}'")),
        };

        let mut last_emit = Instant::now() - Duration::from_secs(1);
        active
            .scanner
            .next_scan_with_progress(&active.process, &filter, |p| {
                let finished = p.total > 0 && p.current >= p.total;
                if finished || last_emit.elapsed() >= Duration::from_millis(60) {
                    last_emit = Instant::now();
                    let percent = if p.total > 0 {
                        ((p.current as f64 / p.total as f64) * 100.0).min(100.0) as f32
                    } else {
                        100.0
                    };
                    let _ = window.emit(
                        "scanner-progress",
                        ScanProgressDto {
                            phase: "next",
                            current: p.current,
                            total: p.total,
                            regions_done: 0,
                            regions_total: 0,
                            hits_found: p.hits_found,
                            percent,
                        },
                    );
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(build_scan_preview(active, 150))
    })
    .await
    .map_err(|e| format!("scan task failed: {e}"))?
}

#[tauri::command]
fn scanner_refresh_preview(state: State<'_, AppState>) -> Result<ScanResponse, String> {
    let mut guard = state.scanner.lock().unwrap();
    let active = guard.as_mut().ok_or("Pick a running process first.")?;
    active.scanner.refresh_preview(&active.process, 150);
    Ok(build_scan_preview(active, 150))
}

#[tauri::command]
fn scanner_write_address(
    state: State<'_, AppState>,
    address: String,
    value_type: ValueTypeSchema,
    value: ScalarValue,
) -> Result<(), String> {
    let guard = state.scanner.lock().unwrap();
    let active = guard.as_ref().ok_or("Pick a running process first.")?;
    let trimmed = address
        .trim()
        .strip_prefix("0x")
        .or_else(|| address.trim().strip_prefix("0X"))
        .unwrap_or(address.trim());
    let addr = usize::from_str_radix(trimmed, 16).map_err(|_| format!("Invalid hex address '{address}'"))?;
    let mem_val = value.to_memory_value(value_type);
    active
        .process
        .write_value(addr, &mem_val)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn scanner_export_trainer(
    state: State<'_, AppState>,
    trainer_id: String,
    game_title: String,
    process_name: String,
    author: String,
    version: String,
    steam_app_id: Option<u32>,
    description: Option<String>,
    entries: Vec<CapturedEntryInput>,
    save_to_library: bool,
) -> Result<ScannerExportResult, String> {
    let scanner_guard = state.scanner.lock().unwrap();

    let mut used = std::collections::HashSet::new();
    let mut built_entries = Vec::with_capacity(entries.len());

    for item in entries {
        let trimmed = item
            .address
            .trim()
            .strip_prefix("0x")
            .or_else(|| item.address.trim().strip_prefix("0X"))
            .unwrap_or(item.address.trim());
        let addr_num = usize::from_str_radix(trimmed, 16)
            .map_err(|_| format!("Invalid hex address '{}'", item.address))?;

        // Automatically use portable ModulePointerChain if the address lives inside a loaded module.
        let address_spec = if let Some(active) = scanner_guard.as_ref() {
            if let Some((module, offset)) = resolve_static_module(&active.process, addr_num) {
                AddressSpec::ModulePointerChain {
                    module,
                    base_offset: format!("0x{offset:X}"),
                    offsets: Vec::new(),
                }
            } else {
                AddressSpec::Absolute {
                    address: format!("0x{addr_num:X}"),
                    offsets: Vec::new(),
                }
            }
        } else {
            AddressSpec::Absolute {
                address: format!("0x{addr_num:X}"),
                offsets: Vec::new(),
            }
        };

        let base_slug: String = item
            .label
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let stem = base_slug.trim_matches('_');
        let stem = if stem.is_empty() { "entry" } else { stem };
        let mut slug = stem.to_string();
        let mut idx = 2;
        while !used.insert(slug.clone()) {
            slug = format!("{stem}_{idx}");
            idx += 1;
        }

        built_entries.push(TrainerEntry {
            id: slug,
            label: item.label,
            category: item.category.filter(|c| !c.trim().is_empty()),
            description: None,
            hotkey: None,
            kind: EntryKind::Value {
                address: address_spec,
                value_type: item.value_type,
                default_value: item.default_value,
                freeze: item.freeze,
                min: None,
                max: None,
                step: None,
            },
        });
    }

    drop(scanner_guard);

    let manifest = TrainerManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: trainer_id.trim().to_string(),
        game: GameTarget {
            title: game_title.trim().to_string(),
            processes: vec![process_name.trim().to_string()],
            steam_app_id,
            game_version: None,
            pointer_width: PointerWidthSchema::Bits64,
        },
        author: author.trim().to_string(),
        version: version.trim().to_string(),
        description: description.filter(|d| !d.trim().is_empty()),
        entries: built_entries,
    };

    manifest.validate().map_err(|e| e.to_string())?;

    if save_to_library {
        state
            .repo
            .import_local_manifest(&manifest)
            .map_err(|e| e.to_string())?;
    }

    let trainer_json = manifest.to_json_pretty().map_err(|e| e.to_string())?;
    let sha256 = compute_sha256_hex(trainer_json.as_bytes());
    let (ct_xml, _) = export_ct_xml(&manifest).map_err(|e| e.to_string())?;

    let index_item = RepoTrainerItem {
        id: manifest.id.clone(),
        title: manifest.game.title.clone(),
        processes: manifest.game.processes.clone(),
        steam_app_id: manifest.game.steam_app_id,
        game_version: manifest.game.game_version.clone(),
        trainer_version: manifest.version.clone(),
        author: manifest.author.clone(),
        path: format!("trainers/{}.trainer.json", manifest.id),
        sha256: Some(sha256.clone()),
        description: manifest.description.clone(),
    };
    let repo_index_entry = serde_json::to_string_pretty(&index_item).map_err(|e| e.to_string())?;

    Ok(ScannerExportResult {
        trainer_id: manifest.id,
        trainer_json,
        sha256,
        ct_xml,
        repo_index_entry,
    })
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("Only http(s) URLs can be opened externally.".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("failed to open browser: {e}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = url;
        Err("Opening external URLs is only supported on Windows.".to_string())
    }
}

fn default_data_dir() -> PathBuf {
    if let Some(app_data) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(app_data).join("FreePlay")
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".freeplay-data")
    }
}

fn main() {
    let repo = RepoManager::open(default_data_dir()).expect("failed to initialize FreePlay data directory");
    let state = AppState {
        repo,
        active: Mutex::new(None),
        scanner: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            list_repos,
            add_repo,
            remove_repo,
            refresh_repos,
            search_catalog,
            check_updates,
            list_installed,
            load_trainer,
            install_trainer,
            uninstall_trainer,
            import_ct,
            export_ct,
            import_json_manifest,
            list_running_processes,
            attach_trainer,
            attach_trainer_to_pid,
            detach_trainer,
            session_status,
            read_entry,
            write_entry,
            set_freeze,
            toggle_patch,
            scanner_attach,
            scanner_detach,
            scanner_first_scan,
            scanner_next_scan,
            scanner_refresh_preview,
            scanner_write_address,
            scanner_export_trainer,
            open_external_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FreePlay");
}
