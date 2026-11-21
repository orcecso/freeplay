use crate::error::{Result, TrainerError};

#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::mem::{size_of, zeroed};
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, BOOL, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, STILL_ACTIVE,
};
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
    MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub exe_path: Option<String>,
    pub window_title: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub name: String,
    pub base_address: usize,
    pub size: usize,
}

/// RAII wrapper around a Windows process handle opened with VM read/write/operation + query access.
#[derive(Debug)]
pub struct ProcessHandle {
    pid: u32,
    #[cfg(windows)]
    handle: HANDLE,
}

// SAFETY: Windows process HANDLEs are valid across threads in the same process.
unsafe impl Send for ProcessHandle {}
unsafe impl Sync for ProcessHandle {}

#[cfg(windows)]
fn wide_buf_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end])
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn collect_window_titles() -> HashMap<u32, String> {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return 1;
        }
        let mut buf = vec![0u16; (len as usize) + 1];
        let written = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if written <= 0 {
            return 1;
        }
        let title = OsString::from_wide(&buf[..written as usize])
            .to_string_lossy()
            .trim()
            .to_string();
        if title.is_empty() {
            return 1;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            let map = &mut *(lparam as *mut HashMap<u32, String>);
            map.entry(pid).or_insert(title);
        }
        1
    }

    let mut map: HashMap<u32, String> = HashMap::new();
    unsafe {
        EnumWindows(Some(enum_proc), (&mut map as *mut HashMap<u32, String>) as LPARAM);
    }
    map
}

#[cfg(windows)]
fn query_exe_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 || len == 0 {
            return None;
        }
        Some(
            OsString::from_wide(&buf[..len as usize])
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(windows)]
fn is_windows_system_process(name: &str, exe_path: Option<&str>) -> bool {
    if let Some(path) = exe_path {
        let lower = path.to_ascii_lowercase();
        if lower.starts_with("c:\\windows\\")
            || lower.contains("\\windowsapps\\microsoft.")
            || lower.contains("\\systemapps\\")
        {
            return true;
        }
        return false;
    }

    // If we couldn't open even PROCESS_QUERY_LIMITED_INFORMATION, it's almost certainly
    // a protected Windows kernel/system/security process.
    let lower_name = name.to_ascii_lowercase();
    matches!(
        lower_name.as_str(),
        "[system process]"
            | "system"
            | "registry"
            | "smss.exe"
            | "csrss.exe"
            | "wininit.exe"
            | "services.exe"
            | "lsass.exe"
            | "lsaIso.exe"
            | "memcompression"
            | "svchost.exe"
            | "dwm.exe"
            | "fontdrvhost.exe"
            | "audiodg.exe"
            | "msmpeng.exe"
            | "nissrv.exe"
            | "securityhealthservice.exe"
            | "sgrmbroker.exe"
            | "spoolsv.exe"
            | "wudfhost.exe"
    ) || true
}

#[cfg(windows)]
pub fn list_processes() -> Result<Vec<ProcessInfo>> {
    let titles = collect_window_titles();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(TrainerError::WindowsApi {
                context: "CreateToolhelp32Snapshot(PROCESS)",
                code: GetLastError(),
            });
        }

        let mut results = Vec::new();
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let pid = entry.th32ProcessID;
                let name = wide_buf_to_string(&entry.szExeFile);
                let exe_path = if pid > 4 { query_exe_path(pid) } else { None };
                let is_system = is_windows_system_process(&name, exe_path.as_deref());
                let window_title = titles.get(&pid).cloned();
                results.push(ProcessInfo {
                    pid,
                    name,
                    exe_path,
                    window_title,
                    is_system,
                });
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        Ok(results)
    }
}

#[cfg(not(windows))]
pub fn list_processes() -> Result<Vec<ProcessInfo>> {
    Err(TrainerError::UnsupportedPlatform)
}

/// Case-insensitive match against executable name (e.g. `"dummy-target.exe"`).
pub fn find_processes_by_name(exe_name: &str) -> Result<Vec<ProcessInfo>> {
    let target = exe_name.to_ascii_lowercase();
    let procs = list_processes()?;
    Ok(procs
        .into_iter()
        .filter(|p| p.name.to_ascii_lowercase() == target)
        .collect())
}

impl ProcessHandle {
    #[cfg(windows)]
    pub fn attach_pid(pid: u32) -> Result<Self> {
        crate::guard::ensure_safe_to_attach(pid)?;

        let access = PROCESS_VM_READ
            | PROCESS_VM_WRITE
            | PROCESS_VM_OPERATION
            | PROCESS_QUERY_INFORMATION;
        let process = unsafe {
            let handle = OpenProcess(access, 0, pid);
            if handle.is_null() {
                return Err(TrainerError::WindowsApi {
                    context: "OpenProcess",
                    code: GetLastError(),
                });
            }
            Self { pid, handle }
        };

        if let Ok(modules) = process.list_modules() {
            crate::guard::ensure_no_anticheat_modules(&modules)?;
        }

        Ok(process)
    }

    #[cfg(not(windows))]
    pub fn attach_pid(_pid: u32) -> Result<Self> {
        Err(TrainerError::UnsupportedPlatform)
    }

    pub fn attach_by_name(exe_name: &str) -> Result<Self> {
        let matches = find_processes_by_name(exe_name)?;
        let proc_info = matches
            .into_iter()
            .next()
            .ok_or_else(|| TrainerError::ProcessNotFound(exe_name.to_string()))?;
        Self::attach_pid(proc_info.pid)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    #[cfg(windows)]
    pub(crate) fn raw_handle(&self) -> HANDLE {
        self.handle
    }

    #[cfg(windows)]
    pub fn is_alive(&self) -> bool {
        unsafe {
            let mut exit_code: u32 = 0;
            if GetExitCodeProcess(self.handle, &mut exit_code) == 0 {
                return false;
            }
            exit_code == STILL_ACTIVE as u32
        }
    }

    #[cfg(not(windows))]
    pub fn is_alive(&self) -> bool {
        false
    }

    #[cfg(windows)]
    pub fn list_modules(&self) -> Result<Vec<ModuleInfo>> {
        unsafe {
            let snapshot =
                CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.pid);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(TrainerError::WindowsApi {
                    context: "CreateToolhelp32Snapshot(MODULE)",
                    code: GetLastError(),
                });
            }

            let mut results = Vec::new();
            let mut entry: MODULEENTRY32W = zeroed();
            entry.dwSize = size_of::<MODULEENTRY32W>() as u32;

            if Module32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    results.push(ModuleInfo {
                        name: wide_buf_to_string(&entry.szModule),
                        base_address: entry.modBaseAddr as usize,
                        size: entry.modBaseSize as usize,
                    });
                    if Module32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }

            CloseHandle(snapshot);
            Ok(results)
        }
    }

    #[cfg(not(windows))]
    pub fn list_modules(&self) -> Result<Vec<ModuleInfo>> {
        Err(TrainerError::UnsupportedPlatform)
    }

    /// Look up the base address of a loaded module (case-insensitive, e.g. `"game.exe"` or `"mono.dll"`).
    pub fn module_base(&self, module_name: &str) -> Result<usize> {
        let target = module_name.to_ascii_lowercase();
        let modules = self.list_modules()?;
        modules
            .into_iter()
            .find(|m| m.name.to_ascii_lowercase() == target)
            .map(|m| m.base_address)
            .ok_or_else(|| TrainerError::ModuleNotFound(module_name.to_string()))
    }
}

#[cfg(windows)]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}
