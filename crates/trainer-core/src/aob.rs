use crate::error::{Result, TrainerError};
use crate::process::ProcessHandle;

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::mem::{size_of, zeroed};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READ,
    PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY,
    PAGE_READWRITE, PAGE_WRITECOPY,
};

/// A parsed Array-of-Bytes signature, e.g. `"89 43 10 ?? ?? 48 8B"`.
/// `None` represents a wildcard byte (`?`, `??`, `*`, `**`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AobPattern {
    bytes: Vec<Option<u8>>,
}

impl AobPattern {
    pub fn parse(pattern: &str) -> Result<Self> {
        let mut bytes = Vec::new();
        for token in pattern.split_whitespace() {
            if token == "?" || token == "??" || token == "*" || token == "**" {
                bytes.push(None);
            } else {
                let hex = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")).unwrap_or(token);
                if hex.len() != 2 {
                    return Err(TrainerError::InvalidPattern(format!(
                        "token '{token}' must be 2 hex digits or wildcard"
                    )));
                }
                let val = u8::from_str_radix(hex, 16).map_err(|_| {
                    TrainerError::InvalidPattern(format!("invalid hex byte '{token}'"))
                })?;
                bytes.push(Some(val));
            }
        }
        if bytes.is_empty() {
            return Err(TrainerError::InvalidPattern("pattern is empty".into()));
        }
        if bytes.iter().all(|b| b.is_none()) {
            return Err(TrainerError::InvalidPattern(
                "pattern cannot be all wildcards".into(),
            ));
        }
        Ok(Self { bytes })
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Scans a byte slice and returns all matching start offsets within `haystack`.
    pub fn find_all_in(&self, haystack: &[u8]) -> Vec<usize> {
        let pat_len = self.bytes.len();
        if pat_len == 0 || haystack.len() < pat_len {
            return Vec::new();
        }

        // Anchor on the first non-wildcard byte for a fast skip check.
        let (anchor_idx, anchor_byte) = self
            .bytes
            .iter()
            .enumerate()
            .find_map(|(i, b)| b.map(|v| (i, v)))
            .expect("validated non-wildcard byte exists");

        let mut matches = Vec::new();
        let last_start = haystack.len() - pat_len;
        let mut i = 0;
        while i <= last_start {
            if haystack[i + anchor_idx] == anchor_byte {
                let mut matched = true;
                for (j, pat_b) in self.bytes.iter().enumerate() {
                    if let Some(expected) = pat_b {
                        if haystack[i + j] != *expected {
                            matched = false;
                            break;
                        }
                    }
                }
                if matched {
                    matches.push(i);
                }
            }
            i += 1;
        }
        matches
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    /// If true, only scan pages with executable permissions (ideal for code/instruction signatures).
    pub executable_only: bool,
    /// Optional start address (inclusive).
    pub start_address: Option<usize>,
    /// Optional end address (exclusive).
    pub end_address: Option<usize>,
    /// Optional cap on how many matches to return before stopping early.
    pub max_results: Option<usize>,
}

#[cfg(windows)]
fn is_readable_protection(protect: u32, executable_only: bool) -> bool {
    if (protect & PAGE_GUARD) != 0 || (protect & PAGE_NOACCESS) != 0 {
        return false;
    }
    let base = protect & 0xFF;
    if executable_only {
        matches!(
            base,
            PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY
        )
    } else {
        matches!(
            base,
            PAGE_READONLY
                | PAGE_READWRITE
                | PAGE_WRITECOPY
                | PAGE_EXECUTE_READ
                | PAGE_EXECUTE_READWRITE
                | PAGE_EXECUTE_WRITECOPY
        )
    }
}

impl ProcessHandle {
    /// Scans committed, readable memory regions of the process for an AoB pattern.
    #[cfg(windows)]
    pub fn scan_aob(&self, pattern: &AobPattern, opts: ScanOptions) -> Result<Vec<usize>> {
        let mut results = Vec::new();
        let mut cursor = opts.start_address.unwrap_or(0);
        let end_limit = opts.end_address.unwrap_or(usize::MAX);

        while cursor < end_limit {
            let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { zeroed() };
            let queried = unsafe {
                VirtualQueryEx(
                    self.raw_handle(),
                    cursor as *const c_void,
                    &mut mbi,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if queried == 0 {
                break;
            }

            let region_base = mbi.BaseAddress as usize;
            let region_size = mbi.RegionSize;
            if region_size == 0 {
                break;
            }

            let next_cursor = region_base.saturating_add(region_size);

            if mbi.State == MEM_COMMIT && is_readable_protection(mbi.Protect, opts.executable_only) {
                let read_start = region_base.max(cursor);
                let read_end = next_cursor.min(end_limit);
                if read_end > read_start {
                    let read_len = read_end - read_start;
                    // Ignore regions that fail to read (e.g. concurrently unmapped pages).
                    if let Ok(buf) = self.read_bytes(read_start, read_len) {
                        for rel_offset in pattern.find_all_in(&buf) {
                            results.push(read_start + rel_offset);
                            if let Some(max) = opts.max_results {
                                if results.len() >= max {
                                    return Ok(results);
                                }
                            }
                        }
                    }
                }
            }

            if next_cursor <= cursor {
                break;
            }
            cursor = next_cursor;
        }

        Ok(results)
    }

    #[cfg(not(windows))]
    pub fn scan_aob(&self, _pattern: &AobPattern, _opts: ScanOptions) -> Result<Vec<usize>> {
        Err(TrainerError::UnsupportedPlatform)
    }

    /// Convenience helper: scans only within a specific loaded module's address range.
    pub fn scan_module_aob(
        &self,
        module: &str,
        pattern: &AobPattern,
        executable_only: bool,
    ) -> Result<Vec<usize>> {
        let target = module.to_ascii_lowercase();
        let info = self
            .list_modules()?
            .into_iter()
            .find(|m| m.name.to_ascii_lowercase() == target)
            .ok_or_else(|| TrainerError::ModuleNotFound(module.to_string()))?;

        self.scan_aob(
            pattern,
            ScanOptions {
                executable_only,
                start_address: Some(info.base_address),
                end_address: Some(info.base_address.saturating_add(info.size)),
                max_results: None,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_match_wildcards() {
        let pat = AobPattern::parse("DE AD ?? BE ? EF").unwrap();
        let data = [
            0x00, 0xDE, 0xAD, 0x11, 0xBE, 0x22, 0xEF, 0x99, 0xDE, 0xAD, 0xFF, 0xBE, 0x00, 0xEF,
        ];
        assert_eq!(pat.find_all_in(&data), vec![1, 8]);
    }

    #[test]
    fn reject_invalid_patterns() {
        assert!(AobPattern::parse("").is_err());
        assert!(AobPattern::parse("?? ??").is_err());
        assert!(AobPattern::parse("ZZ 10").is_err());
        assert!(AobPattern::parse("123").is_err());
    }
}
