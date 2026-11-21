use crate::error::{Result, TrainerError};
use crate::process::ProcessHandle;

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use windows_sys::Win32::Foundation::GetLastError;
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{VirtualProtectEx, PAGE_EXECUTE_READWRITE};

/// A reversible byte patch at a target address.
/// Saves the original bytes before applying so disabling cleanly restores memory.
#[derive(Debug, Clone)]
pub struct BytePatch {
    address: usize,
    patch_bytes: Vec<u8>,
    original_bytes: Option<Vec<u8>>,
}

impl BytePatch {
    pub fn new(address: usize, patch_bytes: Vec<u8>) -> Self {
        Self {
            address,
            patch_bytes,
            original_bytes: None,
        }
    }

    pub fn address(&self) -> usize {
        self.address
    }

    pub fn is_applied(&self) -> bool {
        self.original_bytes.is_some()
    }

    pub fn original_bytes(&self) -> Option<&[u8]> {
        self.original_bytes.as_deref()
    }

    /// Reads and saves the current bytes, temporarily adjusts page protection if needed,
    /// and writes `patch_bytes`.
    pub fn apply(&mut self, process: &ProcessHandle) -> Result<()> {
        if self.is_applied() {
            return Err(TrainerError::PatchAlreadyApplied);
        }
        let original = process.read_bytes(self.address, self.patch_bytes.len())?;
        write_with_protect(process, self.address, &self.patch_bytes)?;
        self.original_bytes = Some(original);
        Ok(())
    }

    /// Restores the saved original bytes and marks the patch inactive.
    pub fn revert(&mut self, process: &ProcessHandle) -> Result<()> {
        let original = self
            .original_bytes
            .take()
            .ok_or(TrainerError::PatchNotApplied)?;
        write_with_protect(process, self.address, &original)?;
        Ok(())
    }
}

#[cfg(windows)]
fn write_with_protect(process: &ProcessHandle, address: usize, data: &[u8]) -> Result<()> {
    let mut old_protect: u32 = 0;
    unsafe {
        let ok = VirtualProtectEx(
            process.raw_handle(),
            address as *const c_void,
            data.len(),
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        );
        if ok == 0 {
            return Err(TrainerError::WindowsApi {
                context: "VirtualProtectEx(enable)",
                code: GetLastError(),
            });
        }
    }

    let write_res = process.write_bytes(address, data);

    unsafe {
        let mut _ignored: u32 = 0;
        let _ = VirtualProtectEx(
            process.raw_handle(),
            address as *const c_void,
            data.len(),
            old_protect,
            &mut _ignored,
        );
    }

    write_res
}

#[cfg(not(windows))]
fn write_with_protect(_process: &ProcessHandle, _address: usize, _data: &[u8]) -> Result<()> {
    Err(TrainerError::UnsupportedPlatform)
}
