use crate::error::{Result, TrainerError};
use crate::process::ProcessHandle;

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::mem::{size_of, zeroed};
#[cfg(windows)]
use windows_sys::Win32::Foundation::GetLastError;
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_FREE,
    MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
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

impl ValueType {
    pub fn byte_len(self) -> usize {
        match self {
            ValueType::U8 => 1,
            ValueType::I16 | ValueType::U16 => 2,
            ValueType::I32 | ValueType::U32 | ValueType::F32 => 4,
            ValueType::I64 | ValueType::U64 | ValueType::F64 => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryValue {
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Bytes(Vec<u8>),
}

impl MemoryValue {
    pub fn to_le_bytes(&self) -> Vec<u8> {
        match self {
            MemoryValue::U8(v) => vec![*v],
            MemoryValue::I16(v) => v.to_le_bytes().to_vec(),
            MemoryValue::U16(v) => v.to_le_bytes().to_vec(),
            MemoryValue::I32(v) => v.to_le_bytes().to_vec(),
            MemoryValue::U32(v) => v.to_le_bytes().to_vec(),
            MemoryValue::I64(v) => v.to_le_bytes().to_vec(),
            MemoryValue::U64(v) => v.to_le_bytes().to_vec(),
            MemoryValue::F32(v) => v.to_le_bytes().to_vec(),
            MemoryValue::F64(v) => v.to_le_bytes().to_vec(),
            MemoryValue::Bytes(b) => b.clone(),
        }
    }

    pub fn from_le_bytes(ty: ValueType, bytes: &[u8]) -> Result<Self> {
        let expected = ty.byte_len();
        if bytes.len() != expected {
            return Err(TrainerError::SizeMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(match ty {
            ValueType::U8 => MemoryValue::U8(bytes[0]),
            ValueType::I16 => MemoryValue::I16(i16::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::U16 => MemoryValue::U16(u16::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::I32 => MemoryValue::I32(i32::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::U32 => MemoryValue::U32(u32::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::I64 => MemoryValue::I64(i64::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::U64 => MemoryValue::U64(u64::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::F32 => MemoryValue::F32(f32::from_le_bytes(bytes.try_into().unwrap())),
            ValueType::F64 => MemoryValue::F64(f64::from_le_bytes(bytes.try_into().unwrap())),
        })
    }
}

/// Pointer size of the target process for pointer-chain traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerWidth {
    Bits32,
    Bits64,
}

impl ProcessHandle {
    #[cfg(windows)]
    pub fn read_bytes(&self, address: usize, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut bytes_read: usize = 0;
        unsafe {
            let ok = ReadProcessMemory(
                self.raw_handle(),
                address as *const c_void,
                buf.as_mut_ptr() as *mut c_void,
                len,
                &mut bytes_read,
            );
            if ok == 0 {
                return Err(TrainerError::WindowsApi {
                    context: "ReadProcessMemory",
                    code: GetLastError(),
                });
            }
        }
        buf.truncate(bytes_read);
        if buf.len() != len {
            return Err(TrainerError::SizeMismatch {
                expected: len,
                actual: buf.len(),
            });
        }
        Ok(buf)
    }

    #[cfg(target_os = "linux")]
    pub fn read_bytes(&self, address: usize, len: usize) -> Result<Vec<u8>> {
        crate::linux::read_bytes_linux(self.pid(), self.mem_file(), address, len)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn read_bytes(&self, _address: usize, _len: usize) -> Result<Vec<u8>> {
        Err(TrainerError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn write_bytes(&self, address: usize, data: &[u8]) -> Result<()> {
        let mut bytes_written: usize = 0;
        unsafe {
            let ok = WriteProcessMemory(
                self.raw_handle(),
                address as *mut c_void,
                data.as_ptr() as *const c_void,
                data.len(),
                &mut bytes_written,
            );
            if ok == 0 {
                return Err(TrainerError::WindowsApi {
                    context: "WriteProcessMemory",
                    code: GetLastError(),
                });
            }
        }
        if bytes_written != data.len() {
            return Err(TrainerError::SizeMismatch {
                expected: data.len(),
                actual: bytes_written,
            });
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn write_bytes(&self, address: usize, data: &[u8]) -> Result<()> {
        crate::linux::write_bytes_linux(self.pid(), self.mem_file(), address, data, false)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn write_bytes(&self, _address: usize, _data: &[u8]) -> Result<()> {
        Err(TrainerError::UnsupportedPlatform)
    }

    pub fn read_value(&self, address: usize, ty: ValueType) -> Result<MemoryValue> {
        let bytes = self.read_bytes(address, ty.byte_len())?;
        MemoryValue::from_le_bytes(ty, &bytes)
    }

    pub fn write_value(&self, address: usize, value: &MemoryValue) -> Result<()> {
        let bytes = value.to_le_bytes();
        self.write_bytes(address, &bytes)
    }

    /// Resolves a multi-level pointer chain starting from `base_address`.
    ///
    /// For each offset in `offsets`:
    /// - Reads a pointer from the current address (using `width`).
    /// - Checks for null.
    /// - Adds the signed offset (`isize`) to obtain the next address.
    ///
    /// Example: `offsets = &[0x10, 0x20]` computes `*(*base_address + 0x10) + 0x20`.
    pub fn resolve_pointer_chain(
        &self,
        base_address: usize,
        offsets: &[isize],
        width: PointerWidth,
    ) -> Result<usize> {
        let mut current = base_address;
        for (step, &offset) in offsets.iter().enumerate() {
            let ptr = match width {
                PointerWidth::Bits32 => {
                    let raw = self.read_bytes(current, 4)?;
                    u32::from_le_bytes(raw.try_into().unwrap()) as usize
                }
                PointerWidth::Bits64 => {
                    let raw = self.read_bytes(current, 8)?;
                    u64::from_le_bytes(raw.try_into().unwrap()) as usize
                }
            };
            if ptr == 0 {
                return Err(TrainerError::NullPointer { step });
            }
            current = ptr.wrapping_add_signed(offset);
        }
        Ok(current)
    }

    /// Convenience helper: resolves `module_base(module) + base_offset`, then walks `offsets`.
    pub fn resolve_module_pointer_chain(
        &self,
        module: &str,
        base_offset: usize,
        offsets: &[isize],
        width: PointerWidth,
    ) -> Result<usize> {
        let base = self.module_base(module)?;
        self.resolve_pointer_chain(base.wrapping_add(base_offset), offsets, width)
    }

    /// Allocates executable RWX memory in the target process, preferring a page within
    /// ±1.5 GiB of `near_address` (when `near_address != 0`) so 5-byte relative `jmp rel32`
    /// instructions can reach the allocation.
    #[cfg(windows)]
    pub fn alloc_near(&self, near_address: usize, size: usize) -> Result<usize> {
        let alloc_size = size.max(0x1000);
        if near_address != 0 {
            const GRANULARITY: usize = 0x10000;
            const MAX_DELTA: usize = 0x6000_0000; // 1.5 GiB (safely inside ±2 GiB rel32)

            let min_addr = near_address.saturating_sub(MAX_DELTA).max(GRANULARITY);
            let max_addr = near_address.saturating_add(MAX_DELTA);

            // Search upward from near_address first, then downward from min_addr.
            for &(start, end) in &[(near_address, max_addr), (min_addr, near_address)] {
                let mut cursor = (start + GRANULARITY - 1) & !(GRANULARITY - 1);
                while cursor < end {
                    let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { zeroed() };
                    let q = unsafe {
                        VirtualQueryEx(
                            self.raw_handle(),
                            cursor as *const c_void,
                            &mut mbi,
                            size_of::<MEMORY_BASIC_INFORMATION>(),
                        )
                    };
                    if q == 0 || mbi.RegionSize == 0 {
                        break;
                    }
                    let base = mbi.BaseAddress as usize;
                    let region_end = base.saturating_add(mbi.RegionSize);
                    if mbi.State == MEM_FREE {
                        let aligned = (base.max(cursor) + GRANULARITY - 1) & !(GRANULARITY - 1);
                        if aligned < end && region_end.saturating_sub(aligned) >= alloc_size {
                            let ptr = unsafe {
                                VirtualAllocEx(
                                    self.raw_handle(),
                                    aligned as *const c_void,
                                    alloc_size,
                                    MEM_COMMIT | MEM_RESERVE,
                                    PAGE_EXECUTE_READWRITE,
                                )
                            };
                            if !ptr.is_null() {
                                return Ok(ptr as usize);
                            }
                        }
                    }
                    if region_end <= cursor {
                        break;
                    }
                    cursor = region_end;
                }
            }
        }

        // Fallback: let Windows choose any address.
        let ptr = unsafe {
            VirtualAllocEx(
                self.raw_handle(),
                std::ptr::null(),
                alloc_size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            )
        };
        if ptr.is_null() {
            return Err(TrainerError::WindowsApi {
                context: "VirtualAllocEx",
                code: unsafe { GetLastError() },
            });
        }
        Ok(ptr as usize)
    }

    #[cfg(target_os = "linux")]
    pub fn alloc_near(&self, _near_address: usize, _size: usize) -> Result<usize> {
        Err(TrainerError::UnsupportedOperation(
            "Auto Assembler remote code caves (alloc) are not yet supported on Linux; \
             Value entries, Pointer Chains, Freezing, and AoB Byte Patches work normally."
                .into(),
        ))
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn alloc_near(&self, _near_address: usize, _size: usize) -> Result<usize> {
        Err(TrainerError::UnsupportedPlatform)
    }

    /// Releases memory previously allocated by `alloc_near`.
    #[cfg(windows)]
    pub fn free_memory(&self, address: usize) -> Result<()> {
        let ok = unsafe { VirtualFreeEx(self.raw_handle(), address as *mut c_void, 0, MEM_RELEASE) };
        if ok == 0 {
            return Err(TrainerError::WindowsApi {
                context: "VirtualFreeEx",
                code: unsafe { GetLastError() },
            });
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn free_memory(&self, _address: usize) -> Result<()> {
        Err(TrainerError::UnsupportedOperation(
            "Remote memory release is not yet supported on Linux.".into(),
        ))
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn free_memory(&self, _address: usize) -> Result<()> {
        Err(TrainerError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_memory_values() {
        let cases = [
            (ValueType::I32, MemoryValue::I32(-42)),
            (ValueType::U32, MemoryValue::U32(999_999)),
            (ValueType::F32, MemoryValue::F32(123.5)),
            (ValueType::F64, MemoryValue::F64(98765.4321)),
            (ValueType::I64, MemoryValue::I64(-1_000_000_000_000)),
        ];
        for (ty, val) in cases {
            let bytes = val.to_le_bytes();
            let decoded = MemoryValue::from_le_bytes(ty, &bytes).unwrap();
            assert_eq!(decoded, val);
        }
    }
}
