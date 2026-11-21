use crate::error::Result;
#[cfg(not(windows))]
use crate::error::TrainerError;
use crate::memory::{MemoryValue, ValueType};
use crate::process::ProcessHandle;

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::mem::{size_of, zeroed};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READWRITE, PAGE_WRITECOPY,
};

pub const DEFAULT_MAX_SCAN_HITS: usize = 500_000;

#[derive(Debug, Clone)]
pub enum ScanFilter {
    Exact(MemoryValue),
    /// Matches a numeric value converted to each hit's own `value_type` (used in `All` mode).
    ExactNumber(f64),
    Increased,
    Decreased,
    Changed,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct ScanHit {
    pub address: usize,
    pub value_type: ValueType,
    pub value: MemoryValue,
    pub previous: Option<MemoryValue>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanProgress {
    /// Bytes scanned so far (for First Scan) or candidate items checked (for Next Scan).
    pub current: usize,
    /// Total bytes to scan (for First Scan) or total candidates (for Next Scan).
    pub total: usize,
    /// Memory regions scanned so far (0 for Next Scan).
    pub regions_done: usize,
    /// Total writable memory regions (0 for Next Scan).
    pub regions_total: usize,
    /// Matching hits found so far.
    pub hits_found: usize,
}

/// Stateful live memory value scanner (`First Scan` -> `Next Scan` -> narrow down -> edit/export).
pub struct MemoryScanner {
    value_type: ValueType,
    hits: Vec<ScanHit>,
    max_hits: usize,
}

impl MemoryScanner {
    pub fn new(value_type: ValueType) -> Self {
        Self {
            value_type,
            hits: Vec::new(),
            max_hits: DEFAULT_MAX_SCAN_HITS,
        }
    }

    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub fn hits(&self) -> &[ScanHit] {
        &self.hits
    }

    pub fn total_hits(&self) -> usize {
        self.hits.len()
    }

    pub fn clear(&mut self) {
        self.hits.clear();
    }

    /// Performs an initial scan across all committed, writable memory regions of `process`
    /// for values matching `target`.
    pub fn first_scan(&mut self, process: &ProcessHandle, target: &MemoryValue) -> Result<usize> {
        self.first_scan_with_progress(process, target, |_| {})
    }

    /// Same as `first_scan`, invoking `on_progress` periodically with bytes/regions/hits counts.
    #[cfg(windows)]
    pub fn first_scan_with_progress<F>(
        &mut self,
        process: &ProcessHandle,
        target: &MemoryValue,
        mut on_progress: F,
    ) -> Result<usize>
    where
        F: FnMut(ScanProgress),
    {
        self.hits.clear();
        let stride = self.value_type.byte_len();

        // Pass 1: enumerate committed writable regions so we know exact total bytes & region count.
        let mut regions: Vec<(usize, usize)> = Vec::new();
        let mut total_bytes: usize = 0;
        let mut cursor: usize = 0;

        loop {
            let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { zeroed() };
            let q = unsafe {
                VirtualQueryEx(
                    process.raw_handle(),
                    cursor as *const c_void,
                    &mut mbi,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if q == 0 || mbi.RegionSize == 0 {
                break;
            }
            let base = mbi.BaseAddress as usize;
            let region_size = mbi.RegionSize;
            let next_cursor = base.saturating_add(region_size);

            if mbi.State == MEM_COMMIT && is_writable_protection(mbi.Protect) {
                regions.push((base, region_size));
                total_bytes = total_bytes.saturating_add(region_size);
            }
            if next_cursor <= cursor {
                break;
            }
            cursor = next_cursor;
        }

        let regions_total = regions.len();
        let mut scanned_bytes: usize = 0;
        on_progress(ScanProgress {
            current: 0,
            total: total_bytes,
            regions_done: 0,
            regions_total,
            hits_found: 0,
        });

        // Pass 2: read each region in 4 MiB chunks and report progress.
        const CHUNK_SIZE: usize = 4 * 1024 * 1024;
        for (idx, (base, region_size)) in regions.into_iter().enumerate() {
            if self.hits.len() >= self.max_hits {
                break;
            }
            let mut offset = 0usize;
            while offset < region_size && self.hits.len() < self.max_hits {
                let len = (region_size - offset).min(CHUNK_SIZE);
                let chunk_addr = base + offset;
                if let Ok(buf) = process.read_bytes(chunk_addr, len) {
                    let mut i = 0usize;
                    while i + stride <= buf.len() {
                        if let Ok(val) =
                            MemoryValue::from_le_bytes(self.value_type, &buf[i..i + stride])
                        {
                            if values_match(&val, target) {
                                self.hits.push(ScanHit {
                                    address: chunk_addr + i,
                                    value_type: self.value_type,
                                    value: val,
                                    previous: None,
                                });
                                if self.hits.len() >= self.max_hits {
                                    break;
                                }
                            }
                        }
                        i += stride;
                    }
                }
                offset += len;
                scanned_bytes = scanned_bytes.saturating_add(len);
                on_progress(ScanProgress {
                    current: scanned_bytes,
                    total: total_bytes,
                    regions_done: idx + 1,
                    regions_total,
                    hits_found: self.hits.len(),
                });
            }
        }

        on_progress(ScanProgress {
            current: total_bytes,
            total: total_bytes,
            regions_done: regions_total,
            regions_total,
            hits_found: self.hits.len(),
        });

        Ok(self.hits.len())
    }

    /// Scans writable memory in a single pass for `i32` (4 Bytes), `f32` (Float), and `f64` (Double)
    /// simultaneously—matching Cheat Engine's "All" value type mode.
    #[cfg(windows)]
    pub fn first_scan_all_with_progress<F>(
        &mut self,
        process: &ProcessHandle,
        target_num: f64,
        mut on_progress: F,
    ) -> Result<usize>
    where
        F: FnMut(ScanProgress),
    {
        self.hits.clear();

        let target_i32 = if target_num.fract().abs() < 1e-6
            && target_num >= i32::MIN as f64
            && target_num <= i32::MAX as f64
        {
            Some(target_num.round() as i32)
        } else {
            None
        };
        let target_f32 = target_num as f32;

        // Pass 1: enumerate committed writable regions
        let mut regions: Vec<(usize, usize)> = Vec::new();
        let mut total_bytes: usize = 0;
        let mut cursor: usize = 0;

        loop {
            let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { zeroed() };
            let q = unsafe {
                VirtualQueryEx(
                    process.raw_handle(),
                    cursor as *const c_void,
                    &mut mbi,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if q == 0 || mbi.RegionSize == 0 {
                break;
            }
            let base = mbi.BaseAddress as usize;
            let region_size = mbi.RegionSize;
            let next_cursor = base.saturating_add(region_size);

            if mbi.State == MEM_COMMIT && is_writable_protection(mbi.Protect) {
                regions.push((base, region_size));
                total_bytes = total_bytes.saturating_add(region_size);
            }
            if next_cursor <= cursor {
                break;
            }
            cursor = next_cursor;
        }

        let regions_total = regions.len();
        let mut scanned_bytes: usize = 0;
        on_progress(ScanProgress {
            current: 0,
            total: total_bytes,
            regions_done: 0,
            regions_total,
            hits_found: 0,
        });

        const CHUNK_SIZE: usize = 4 * 1024 * 1024;
        for (idx, (base, region_size)) in regions.into_iter().enumerate() {
            if self.hits.len() >= self.max_hits {
                break;
            }
            let mut offset = 0usize;
            while offset < region_size && self.hits.len() < self.max_hits {
                let len = (region_size - offset).min(CHUNK_SIZE);
                let chunk_addr = base + offset;
                if let Ok(buf) = process.read_bytes(chunk_addr, len) {
                    let mut i = 0usize;
                    while i + 4 <= buf.len() && self.hits.len() < self.max_hits {
                        let b4: [u8; 4] = buf[i..i + 4].try_into().unwrap();
                        if let Some(ti) = target_i32 {
                            let v = i32::from_le_bytes(b4);
                            if v == ti {
                                self.hits.push(ScanHit {
                                    address: chunk_addr + i,
                                    value_type: ValueType::I32,
                                    value: MemoryValue::I32(v),
                                    previous: None,
                                });
                            }
                        }
                        if self.hits.len() < self.max_hits {
                            let vf = f32::from_le_bytes(b4);
                            if vf.is_finite() && (vf - target_f32).abs() <= 0.5 {
                                self.hits.push(ScanHit {
                                    address: chunk_addr + i,
                                    value_type: ValueType::F32,
                                    value: MemoryValue::F32(vf),
                                    previous: None,
                                });
                            }
                        }
                        if i % 8 == 0 && i + 8 <= buf.len() && self.hits.len() < self.max_hits {
                            let b8: [u8; 8] = buf[i..i + 8].try_into().unwrap();
                            let vd = f64::from_le_bytes(b8);
                            if vd.is_finite() && (vd - target_num).abs() <= 0.5 {
                                self.hits.push(ScanHit {
                                    address: chunk_addr + i,
                                    value_type: ValueType::F64,
                                    value: MemoryValue::F64(vd),
                                    previous: None,
                                });
                            }
                        }
                        i += 4;
                    }
                }
                offset += len;
                scanned_bytes = scanned_bytes.saturating_add(len);
                on_progress(ScanProgress {
                    current: scanned_bytes,
                    total: total_bytes,
                    regions_done: idx + 1,
                    regions_total,
                    hits_found: self.hits.len(),
                });
            }
        }

        on_progress(ScanProgress {
            current: total_bytes,
            total: total_bytes,
            regions_done: regions_total,
            regions_total,
            hits_found: self.hits.len(),
        });

        Ok(self.hits.len())
    }

    #[cfg(not(windows))]
    pub fn first_scan_all_with_progress<F>(
        &mut self,
        _process: &ProcessHandle,
        _target_num: f64,
        _on_progress: F,
    ) -> Result<usize>
    where
        F: FnMut(ScanProgress),
    {
        Err(TrainerError::UnsupportedPlatform)
    }

    #[cfg(not(windows))]
    pub fn first_scan_with_progress<F>(
        &mut self,
        _process: &ProcessHandle,
        _target: &MemoryValue,
        _on_progress: F,
    ) -> Result<usize>
    where
        F: FnMut(ScanProgress),
    {
        Err(TrainerError::UnsupportedPlatform)
    }

    /// Narrows the existing candidate list using `filter`.
    pub fn next_scan(&mut self, process: &ProcessHandle, filter: &ScanFilter) -> Result<usize> {
        self.next_scan_with_progress(process, filter, |_| {})
    }

    /// Same as `next_scan`, invoking `on_progress` periodically with candidate counts.
    pub fn next_scan_with_progress<F>(
        &mut self,
        process: &ProcessHandle,
        filter: &ScanFilter,
        mut on_progress: F,
    ) -> Result<usize>
    where
        F: FnMut(ScanProgress),
    {
        let total = self.hits.len();
        let mut kept = Vec::with_capacity(total.min(4096));

        on_progress(ScanProgress {
            current: 0,
            total,
            regions_done: 0,
            regions_total: 0,
            hits_found: 0,
        });

        for (i, hit) in self.hits.drain(..).enumerate() {
            if let Ok(current) = process.read_value(hit.address, hit.value_type) {
                let matched = match filter {
                    ScanFilter::Exact(target) => values_match(&current, target),
                    ScanFilter::ExactNumber(num) => {
                        let target = number_to_value(*num, hit.value_type);
                        values_match(&current, &target)
                    }
                    ScanFilter::Increased => {
                        compare_values(&current, &hit.value).is_some_and(|o| o.is_gt())
                    }
                    ScanFilter::Decreased => {
                        compare_values(&current, &hit.value).is_some_and(|o| o.is_lt())
                    }
                    ScanFilter::Changed => !values_exact_eq(&current, &hit.value),
                    ScanFilter::Unchanged => values_exact_eq(&current, &hit.value),
                };
                if matched {
                    kept.push(ScanHit {
                        address: hit.address,
                        value_type: hit.value_type,
                        previous: Some(hit.value),
                        value: current,
                    });
                }
            }

            if (i + 1) % 4096 == 0 {
                on_progress(ScanProgress {
                    current: i + 1,
                    total,
                    regions_done: 0,
                    regions_total: 0,
                    hits_found: kept.len(),
                });
            }
        }

        self.hits = kept;
        on_progress(ScanProgress {
            current: total,
            total,
            regions_done: 0,
            regions_total: 0,
            hits_found: self.hits.len(),
        });
        Ok(self.hits.len())
    }

    /// Re-reads the live values of the first `limit` hits so the UI stays up to date.
    pub fn refresh_preview(&mut self, process: &ProcessHandle, limit: usize) {
        for hit in self.hits.iter_mut().take(limit) {
            if let Ok(val) = process.read_value(hit.address, hit.value_type) {
                hit.value = val;
            }
        }
    }
}

fn number_to_value(num: f64, ty: ValueType) -> MemoryValue {
    match ty {
        ValueType::U8 => MemoryValue::U8(num.round() as u8),
        ValueType::I16 => MemoryValue::I16(num.round() as i16),
        ValueType::U16 => MemoryValue::U16(num.round() as u16),
        ValueType::I32 => MemoryValue::I32(num.round() as i32),
        ValueType::U32 => MemoryValue::U32(num.round() as u32),
        ValueType::I64 => MemoryValue::I64(num.round() as i64),
        ValueType::U64 => MemoryValue::U64(num.round() as u64),
        ValueType::F32 => MemoryValue::F32(num as f32),
        ValueType::F64 => MemoryValue::F64(num),
    }
}

/// Checks if `address` falls inside any loaded module of `process`.
/// If so, returns `(module_name, offset_from_base)` so the address can be exported
/// portably as `"module"+OFFSET`.
pub fn resolve_static_module(process: &ProcessHandle, address: usize) -> Option<(String, usize)> {
    let modules = process.list_modules().ok()?;
    for m in modules {
        let end = m.base_address.saturating_add(m.size);
        if address >= m.base_address && address < end {
            return Some((m.name, address - m.base_address));
        }
    }
    None
}

#[cfg(windows)]
fn is_writable_protection(protect: u32) -> bool {
    if (protect & PAGE_GUARD) != 0 || (protect & PAGE_NOACCESS) != 0 {
        return false;
    }
    let base = protect & 0xFF;
    matches!(
        base,
        PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY
    )
}

fn values_match(candidate: &MemoryValue, target: &MemoryValue) -> bool {
    match (candidate, target) {
        // Games often store floats like 99.8 or 100.2 when displaying "100".
        (MemoryValue::F32(a), MemoryValue::F32(b)) => a.is_finite() && (a - b).abs() <= 0.5,
        (MemoryValue::F64(a), MemoryValue::F64(b)) => a.is_finite() && (a - b).abs() <= 0.5,
        _ => candidate == target,
    }
}

fn values_exact_eq(a: &MemoryValue, b: &MemoryValue) -> bool {
    match (a, b) {
        (MemoryValue::F32(x), MemoryValue::F32(y)) => (x - y).abs() < 1e-4,
        (MemoryValue::F64(x), MemoryValue::F64(y)) => (x - y).abs() < 1e-6,
        _ => a == b,
    }
}

fn compare_values(a: &MemoryValue, b: &MemoryValue) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (MemoryValue::U8(x), MemoryValue::U8(y)) => Some(x.cmp(y)),
        (MemoryValue::I16(x), MemoryValue::I16(y)) => Some(x.cmp(y)),
        (MemoryValue::U16(x), MemoryValue::U16(y)) => Some(x.cmp(y)),
        (MemoryValue::I32(x), MemoryValue::I32(y)) => Some(x.cmp(y)),
        (MemoryValue::U32(x), MemoryValue::U32(y)) => Some(x.cmp(y)),
        (MemoryValue::I64(x), MemoryValue::I64(y)) => Some(x.cmp(y)),
        (MemoryValue::U64(x), MemoryValue::U64(y)) => Some(x.cmp(y)),
        (MemoryValue::F32(x), MemoryValue::F32(y)) => x.partial_cmp(y),
        (MemoryValue::F64(x), MemoryValue::F64(y)) => x.partial_cmp(y),
        _ => None,
    }
}
