//! Runtime safety guard that prevents attaching to processes protected by
//! anti-cheat software. FreePlay is strictly intended for offline
//! single-player games and never attempts to bypass anti-cheat or DRM.

use crate::error::{Result, TrainerError};
use crate::process::{list_processes, ModuleInfo};

/// Anti-cheat executables that only run while a protected game is active.
const ACTIVE_ANTICHEAT_PROCESSES: &[&str] = &[
    "easyanticheat.exe",
    "easyanticheat_eos.exe",
    "start_protected_game.exe",
    "beclient.exe",
    "beclient_x64.exe",
    "gamemon.des",
    "gamemon64.des",
    "xxd-0.xem",
    "xcorona.xem",
    "equ8_anticheat.exe",
];

/// Background anti-cheat services that should never be attached to directly.
const PROTECTED_SERVICE_PROCESSES: &[&str] = &[
    "beservice.exe",
    "bedaisy.exe",
    "vgc.exe",
    "vgtray.exe",
    "faceitclient.exe",
    "faceitservice.exe",
    "esportalclient.exe",
    "punkbustera.exe",
    "punkbusterb.exe",
    "pnkbstra.exe",
    "pnkbstrb.exe",
    "wellbia.exe",
];

/// Known anti-cheat DLLs / shared libraries that may be loaded directly into a game process.
const BLOCKED_MODULE_NAMES: &[&str] = &[
    "easyanticheat_x64.dll",
    "easyanticheat_x86.dll",
    "easyanticheat_x64.so",
    "libeasyanticheat.so",
    "eossdk-win64-shipping.dll",
    "beclient_x64.dll",
    "beclient.dll",
    "beclient_x64.so",
    "libbeclient.so",
    "vgk.dll",
    "vgc.dll",
    "vac.dll",
    "xigncode.dll",
    "npggnt.des",
    "anticheat.dll",
];

/// Returns the matched anti-cheat identifier if `exe_name` is an active per-game anti-cheat.
pub fn match_active_anticheat(exe_name: &str) -> Option<&'static str> {
    let lower = exe_name.to_ascii_lowercase();
    ACTIVE_ANTICHEAT_PROCESSES
        .iter()
        .copied()
        .find(|&blocked| lower == blocked)
}

/// Returns the matched identifier if `exe_name` is any anti-cheat process or service.
pub fn match_blocked_target(exe_name: &str) -> Option<&'static str> {
    if let Some(m) = match_active_anticheat(exe_name) {
        return Some(m);
    }
    let lower = exe_name.to_ascii_lowercase();
    PROTECTED_SERVICE_PROCESSES
        .iter()
        .copied()
        .find(|&blocked| lower == blocked)
}

/// Returns the matched anti-cheat identifier if `module_name` is a known anti-cheat DLL.
pub fn match_blocked_module(module_name: &str) -> Option<&'static str> {
    let lower = module_name.to_ascii_lowercase();
    BLOCKED_MODULE_NAMES
        .iter()
        .copied()
        .find(|&blocked| lower == blocked)
}

/// Verifies that `target_pid` is not an anti-cheat process and no active
/// per-game anti-cheat process is currently running.
pub fn ensure_safe_to_attach(target_pid: u32) -> Result<()> {
    let procs = list_processes()?;
    for proc_info in procs {
        if proc_info.pid == target_pid {
            if let Some(matched) = match_blocked_target(&proc_info.name) {
                return Err(TrainerError::AntiCheatDetected(matched.to_string()));
            }
        }
        if let Some(matched) = match_active_anticheat(&proc_info.name) {
            return Err(TrainerError::AntiCheatDetected(matched.to_string()));
        }
    }
    Ok(())
}

/// Checks a target process's loaded modules for known anti-cheat DLLs.
pub fn ensure_no_anticheat_modules(modules: &[ModuleInfo]) -> Result<()> {
    for module in modules {
        if let Some(matched) = match_blocked_module(&module.name) {
            return Err(TrainerError::AntiCheatDetected(matched.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_active_and_service_anticheat_processes() {
        assert_eq!(
            match_active_anticheat("EasyAntiCheat_EOS.exe"),
            Some("easyanticheat_eos.exe")
        );
        assert_eq!(match_active_anticheat("PnkBstrA.exe"), None);
        assert_eq!(match_blocked_target("PnkBstrA.exe"), Some("pnkbstra.exe"));
        assert_eq!(match_blocked_target("vgc.exe"), Some("vgc.exe"));
        assert_eq!(match_blocked_target("notepad.exe"), None);
    }

    #[test]
    fn detects_known_anticheat_modules() {
        let clean = vec![ModuleInfo {
            name: "game.exe".into(),
            base_address: 0x1000,
            size: 0x2000,
        }];
        assert!(ensure_no_anticheat_modules(&clean).is_ok());

        let protected = vec![
            ModuleInfo {
                name: "game.exe".into(),
                base_address: 0x1000,
                size: 0x2000,
            },
            ModuleInfo {
                name: "BEClient_x64.dll".into(),
                base_address: 0x4000,
                size: 0x1000,
            },
        ];
        assert!(matches!(
            ensure_no_anticheat_modules(&protected),
            Err(TrainerError::AntiCheatDetected(name)) if name == "beclient_x64.dll"
        ));
    }
}
