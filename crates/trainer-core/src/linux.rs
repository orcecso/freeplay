use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;

use crate::error::{Result, TrainerError};
use crate::process::{ModuleInfo, ProcessInfo};

/// A parsed entry from `/proc/<pid>/maps`.
#[derive(Debug, Clone)]
pub(crate) struct MemoryRegion {
    pub start: usize,
    pub end: usize,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub path: Option<String>,
}

impl MemoryRegion {
    pub fn size(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns true if this region is safe to read during memory/AoB scans
    /// (skips kernel virtual pages and hardware device mappings).
    pub fn is_scannable(&self) -> bool {
        if !self.readable || self.size() == 0 {
            return false;
        }
        if let Some(ref p) = self.path {
            if p.starts_with("/dev/") {
                return false;
            }
            if matches!(p.as_str(), "[vvar]" | "[vdso]" | "[vsyscall]" | "[vvar_vclock]") {
                return false;
            }
        }
        true
    }
}

pub(crate) fn permission_hint(pid: u32) -> String {
    format!(
        "Cannot access memory of PID {pid}. On Linux / Steam Deck, grant CAP_SYS_PTRACE to FreePlay \
         (`sudo setcap cap_sys_ptrace=eip /path/to/freeplay-app`), run `install-steamdeck.sh`, or \
         temporarily allow ptrace (`sudo sysctl -w kernel.yama.ptrace_scope=0`)."
    )
}

pub(crate) fn open_proc_mem(pid: u32) -> Result<File> {
    let proc_dir = format!("/proc/{pid}");
    if !Path::new(&proc_dir).exists() {
        return Err(TrainerError::ProcessNotFound(format!("PID {pid}")));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/proc/{pid}/mem"))
        .map_err(|e| match e.raw_os_error() {
            Some(libc::EACCES) | Some(libc::EPERM) => {
                TrainerError::PermissionDenied(permission_hint(pid))
            }
            Some(libc::ENOENT) | Some(libc::ESRCH) => {
                TrainerError::ProcessNotFound(format!("PID {pid}"))
            }
            Some(code) => TrainerError::LinuxApi {
                context: "open(/proc/<pid>/mem)",
                code,
            },
            None => TrainerError::LinuxApi {
                context: "open(/proc/<pid>/mem)",
                code: -1,
            },
        })
}

pub(crate) fn is_process_alive(pid: u32) -> bool {
    let stat_path = format!("/proc/{pid}/stat");
    let Ok(stat) = fs::read_to_string(&stat_path) else {
        return false;
    };
    // Format: `<pid> (<comm>) <state> ...`
    // Find the last `)` so spaces/parens inside `<comm>` don't confuse parsing.
    if let Some(rparen) = stat.rfind(')') {
        if let Some(state_char) = stat[rparen + 1..].trim_start().chars().next() {
            if matches!(state_char, 'Z' | 'X' | 'x') {
                return false;
            }
        }
    }
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

pub(crate) fn read_memory_regions(pid: u32) -> Result<Vec<MemoryRegion>> {
    let content = fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| {
        match e.raw_os_error() {
            Some(libc::EACCES) | Some(libc::EPERM) => {
                TrainerError::PermissionDenied(permission_hint(pid))
            }
            Some(libc::ENOENT) | Some(libc::ESRCH) => {
                TrainerError::ProcessNotFound(format!("PID {pid}"))
            }
            Some(code) => TrainerError::LinuxApi {
                context: "read(/proc/<pid>/maps)",
                code,
            },
            None => TrainerError::LinuxApi {
                context: "read(/proc/<pid>/maps)",
                code: -1,
            },
        }
    })?;

    Ok(parse_proc_maps(&content))
}

pub(crate) fn parse_proc_maps(content: &str) -> Vec<MemoryRegion> {
    let mut regions = Vec::new();
    for line in content.lines() {
        if let Some(region) = parse_maps_line(line) {
            regions.push(region);
        }
    }
    regions
}

fn parse_maps_line(line: &str) -> Option<MemoryRegion> {
    let mut parts = line.split_whitespace();
    let range = parts.next()?;
    let perms = parts.next()?;
    let _offset = parts.next()?;
    let _dev = parts.next()?;
    let _inode = parts.next()?;

    let (start_hex, end_hex) = range.split_once('-')?;
    let start = usize::from_str_radix(start_hex, 16).ok()?;
    let end = usize::from_str_radix(end_hex, 16).ok()?;
    if end <= start {
        return None;
    }

    let pbytes = perms.as_bytes();
    if pbytes.len() < 3 {
        return None;
    }
    let readable = pbytes[0] == b'r';
    let writable = pbytes[1] == b'w';
    let executable = pbytes[2] == b'x';

    // Preserve spaces in file paths (e.g. Steam library / Proton game directories).
    let path = extract_maps_path(line);

    Some(MemoryRegion {
        start,
        end,
        readable,
        writable,
        executable,
        path,
    })
}

fn extract_maps_path(line: &str) -> Option<String> {
    let mut in_space = true;
    let mut field_count = 0usize;
    for (idx, ch) in line.char_indices() {
        let is_ws = ch.is_ascii_whitespace();
        if in_space && !is_ws {
            field_count += 1;
            if field_count == 6 {
                let rest = line[idx..].trim();
                return if rest.is_empty() {
                    None
                } else {
                    Some(rest.to_string())
                };
            }
            in_space = false;
        } else if !in_space && is_ws {
            in_space = true;
        }
    }
    None
}

/// Extracts basename from either Unix (`/`) or Windows/Wine (`\`) paths,
/// stripping any trailing ` (deleted)` marker added by the Linux kernel.
pub(crate) fn path_basename(raw: &str) -> &str {
    let trimmed = raw.trim().strip_suffix(" (deleted)").unwrap_or(raw.trim());
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
}

pub(crate) fn list_modules_from_maps(pid: u32) -> Result<Vec<ModuleInfo>> {
    let regions = read_memory_regions(pid)?;
    Ok(modules_from_regions(&regions))
}

pub(crate) fn modules_from_regions(regions: &[MemoryRegion]) -> Vec<ModuleInfo> {
    // Group file-backed mappings by exact path, extending contiguous/nearby image sections
    // (within 64 MiB) so PE modules loaded under Wine/Proton and ELF libraries report
    // their full base address and image span cleanly.
    const MAX_SECTION_GAP: usize = 64 * 1024 * 1024;

    let mut order: Vec<String> = Vec::new();
    let mut spans: HashMap<String, (usize, usize)> = HashMap::new();

    for r in regions {
        let Some(ref raw_path) = r.path else {
            continue;
        };
        if raw_path.starts_with('[') || raw_path.starts_with("/dev/") || raw_path.starts_with("anon_inode:") {
            continue;
        }
        let cleaned = raw_path
            .trim()
            .strip_suffix(" (deleted)")
            .unwrap_or(raw_path.trim())
            .to_string();
        if cleaned.is_empty() {
            continue;
        }

        if let Some((base, end)) = spans.get_mut(&cleaned) {
            if r.start <= end.saturating_add(MAX_SECTION_GAP) && r.end >= *base {
                if r.start < *base {
                    *base = r.start;
                }
                if r.end > *end {
                    *end = r.end;
                }
            }
        } else {
            order.push(cleaned.clone());
            spans.insert(cleaned, (r.start, r.end));
        }
    }

    order
        .into_iter()
        .filter_map(|path| {
            let (base, end) = spans.get(&path).copied()?;
            let name = path_basename(&path).to_string();
            if name.is_empty() {
                return None;
            }
            Some(ModuleInfo {
                name,
                base_address: base,
                size: end.saturating_sub(base),
            })
        })
        .collect()
}

pub(crate) fn read_bytes_linux(pid: u32, mem_file: &File, address: usize, len: usize) -> Result<Vec<u8>> {
    if len == 0 {
        return Ok(Vec::new());
    }

    let mut buf = vec![0u8; len];
    let local_iov = libc::iovec {
        iov_base: buf.as_mut_ptr() as *mut libc::c_void,
        iov_len: len,
    };
    let remote_iov = libc::iovec {
        iov_base: address as *mut libc::c_void,
        iov_len: len,
    };

    let n = unsafe { libc::process_vm_readv(pid as libc::pid_t, &local_iov, 1, &remote_iov, 1, 0) };
    if n >= 0 {
        let read_len = n as usize;
        if read_len != len {
            return Err(TrainerError::SizeMismatch {
                expected: len,
                actual: read_len,
            });
        }
        return Ok(buf);
    }

    let err = io::Error::last_os_error().raw_os_error().unwrap_or(-1);
    if err == libc::EPERM || err == libc::EACCES {
        // Fallback to already-opened /proc/<pid>/mem before giving up
        if read_exact_at(mem_file, &mut buf, address as u64).is_ok() {
            return Ok(buf);
        }
        return Err(TrainerError::PermissionDenied(permission_hint(pid)));
    }

    // Fallback to /proc/<pid>/mem (e.g. ENOSYS in containers or partial page fault behavior)
    read_exact_at(mem_file, &mut buf, address as u64).map_err(|e| {
        let code = e.raw_os_error().unwrap_or(err);
        if code == libc::EPERM || code == libc::EACCES {
            TrainerError::PermissionDenied(permission_hint(pid))
        } else {
            TrainerError::LinuxApi {
                context: "process_vm_readv / pread",
                code,
            }
        }
    })?;

    Ok(buf)
}

pub(crate) fn write_bytes_linux(
    pid: u32,
    mem_file: &File,
    address: usize,
    data: &[u8],
    force_proc_mem: bool,
) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    if !force_proc_mem {
        let local_iov = libc::iovec {
            iov_base: data.as_ptr() as *mut libc::c_void,
            iov_len: data.len(),
        };
        let remote_iov = libc::iovec {
            iov_base: address as *mut libc::c_void,
            iov_len: data.len(),
        };

        let n = unsafe {
            libc::process_vm_writev(pid as libc::pid_t, &local_iov, 1, &remote_iov, 1, 0)
        };
        if n >= 0 {
            let written = n as usize;
            if written != data.len() {
                return Err(TrainerError::SizeMismatch {
                    expected: data.len(),
                    actual: written,
                });
            }
            return Ok(());
        }
    }

    // Writing via `/proc/<pid>/mem` uses FOLL_FORCE in the kernel, allowing safe patching
    // of read-only / executable `.text` code pages without requiring mprotect/ptrace-stop.
    write_all_at(mem_file, data, address as u64).map_err(|e| match e.raw_os_error() {
        Some(libc::EPERM) | Some(libc::EACCES) => {
            TrainerError::PermissionDenied(permission_hint(pid))
        }
        Some(code) => TrainerError::LinuxApi {
            context: "pwrite(/proc/<pid>/mem)",
            code,
        },
        None => TrainerError::LinuxApi {
            context: "pwrite(/proc/<pid>/mem)",
            code: -1,
        },
    })
}

fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        match file.read_at(buf, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "short read from /proc/<pid>/mem",
                ));
            }
            Ok(n) => {
                offset = offset.saturating_add(n as u64);
                let tmp = buf;
                buf = &mut tmp[n..];
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn write_all_at(file: &File, mut data: &[u8], mut offset: u64) -> io::Result<()> {
    while !data.is_empty() {
        match file.write_at(data, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "short write to /proc/<pid>/mem",
                ));
            }
            Ok(n) => {
                offset = offset.saturating_add(n as u64);
                data = &data[n..];
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Checks if a Linux process is a Proton/Wine Windows executable by inspecting `/proc/<pid>/cmdline`.
/// Returns `(exe_basename, full_cmdline_arg0)` when a `.exe` target is found so existing Windows
/// `.CT` tables and community repo trainers match seamlessly on Steam Deck and Linux desktops.
fn extract_wine_exe_from_cmdline(cmdline_bytes: &[u8]) -> Option<(String, String)> {
    let args: Vec<String> = cmdline_bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if args.is_empty() {
        return None;
    }

    let first = &args[0];
    if first.to_ascii_lowercase().ends_with(".exe") {
        let base = path_basename(first).to_string();
        if !base.is_empty() {
            return Some((base, first.clone()));
        }
    }

    // Also handle explicit `wine64 /path/to/Game.exe` invocations.
    let first_base = path_basename(first).to_ascii_lowercase();
    if matches!(
        first_base.as_str(),
        "wine" | "wine64" | "wine-preloader" | "wine64-preloader"
    ) {
        if let Some(second) = args.get(1) {
            if second.to_ascii_lowercase().ends_with(".exe") {
                let base = path_basename(second).to_string();
                if !base.is_empty() {
                    return Some((base, second.clone()));
                }
            }
        }
    }

    None
}

fn is_wine_helper_exe(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "winedevice.exe"
            | "services.exe"
            | "plugplay.exe"
            | "svchost.exe"
            | "rpcss.exe"
            | "explorer.exe"
            | "conhost.exe"
            | "tabtip.exe"
            | "wineboot.exe"
            | "start.exe"
            | "rundll32.exe"
            | "msiexec.exe"
            | "steam.exe"
            | "steamerrorreporter.exe"
            | "crashhandler.exe"
            | "crashhandler64.exe"
    )
}

fn has_steam_app_env(pid: u32) -> bool {
    let Ok(bytes) = fs::read(format!("/proc/{pid}/environ")) else {
        return false;
    };
    bytes.split(|&b| b == 0).any(|entry| {
        entry.starts_with(b"SteamAppId=")
            || entry.starts_with(b"STEAM_COMPAT_DATA_PATH=")
            || entry.starts_with(b"SteamGameId=")
    })
}

fn is_linux_system_process(
    name: &str,
    exe_path: Option<&str>,
    is_proton_exe: bool,
    cmdline_empty: bool,
    is_steam_game_env: bool,
) -> bool {
    let lower_name = name.to_ascii_lowercase();

    if is_proton_exe {
        return is_wine_helper_exe(&lower_name);
    }

    // Kernel threads have no userspace cmdline or exe symlink.
    if cmdline_empty && exe_path.is_none() {
        return true;
    }

    // Common Linux desktop/OS/Steam-runtime background daemons.
    if matches!(
        lower_name.as_str(),
        "systemd"
            | "init"
            | "dbus-daemon"
            | "dbus-broker"
            | "dbus-broker-launch"
            | "pipewire"
            | "pipewire-pulse"
            | "wireplumber"
            | "pulseaudio"
            | "gamescope"
            | "gamescopereaper"
            | "gamescopectl"
            | "mangoapp"
            | "xwayland"
            | "xorg"
            | "kwin_wayland"
            | "kwin_x11"
            | "plasmashell"
            | "kded5"
            | "kded6"
            | "gnome-shell"
            | "mutter"
            | "sway"
            | "hyprland"
            | "steam"
            | "steamwebhelper"
            | "reaper"
            | "srt-bwrap"
            | "bwrap"
            | "wineserver"
            | "wine64-preloader"
            | "wine-preloader"
            | "bash"
            | "sh"
            | "dash"
            | "zsh"
            | "fish"
            | "tmux"
            | "sshd"
            | "sudo"
            | "su"
            | "polkitd"
            | "networkmanager"
            | "wpa_supplicant"
            | "bluetoothd"
            | "udisksd"
            | "upowerd"
            | "rtkit-daemon"
            | "thermald"
            | "power-profiles-daemon"
            | "avahi-daemon"
            | "cupsd"
            | "cron"
            | "crond"
            | "rsyslogd"
            | "journald"
            | "webkitnetworkprocess"
            | "webkitwebprocess"
            | "webkitgpupprocess"
            | "freeplay-app"
    ) || lower_name.starts_with("systemd-")
        || lower_name.starts_with("xdg-")
        || lower_name.starts_with("gvfs")
        || lower_name.starts_with("at-spi")
        || lower_name.starts_with("ibus-")
        || lower_name.starts_with("kworker/")
        || lower_name.starts_with("ksoftirqd/")
        || lower_name.starts_with("migration/")
        || lower_name.starts_with("pressure-vessel")
    {
        return true;
    }

    if is_steam_game_env {
        return false;
    }

    if let Some(path) = exe_path {
        let lower_path = path.to_ascii_lowercase();
        if lower_path.starts_with("/sbin/")
            || lower_path.starts_with("/usr/sbin/")
            || lower_path.starts_with("/lib/systemd/")
            || lower_path.starts_with("/usr/lib/systemd/")
            || lower_path.starts_with("/usr/libexec/")
            || lower_path.starts_with("/usr/lib/gvfs/")
            || lower_path.starts_with("/usr/lib/bluetooth/")
            || lower_path.starts_with("/usr/lib/polkit")
            || lower_path.starts_with("/usr/lib/xorg/")
            || lower_path.starts_with("/usr/lib/webkit")
            || lower_path.starts_with("/usr/lib/x86_64-linux-gnu/webkit")
        {
            return true;
        }
        // User binaries, games on internal drive or Steam Deck microSD (`/run/media/...`)
        if lower_path.starts_with("/home/")
            || lower_path.starts_with("/run/media/")
            || lower_path.starts_with("/mnt/")
            || lower_path.starts_with("/opt/")
            || lower_path.starts_with("/tmp/")
        {
            return false;
        }
    }

    // If we couldn't read `/proc/<pid>/exe`, it's owned by another user (e.g. root daemon).
    exe_path.is_none()
}

pub(crate) fn list_processes_linux() -> Result<Vec<ProcessInfo>> {
    let entries = fs::read_dir("/proc").map_err(|e| TrainerError::LinuxApi {
        context: "read_dir(/proc)",
        code: e.raw_os_error().unwrap_or(-1),
    })?;

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid_str) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };

        let proc_path = entry.path();
        let cmdline_bytes = fs::read(proc_path.join("cmdline")).unwrap_or_default();
        let comm = fs::read_to_string(proc_path.join("comm"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if comm.is_empty() && cmdline_bytes.is_empty() {
            continue;
        }

        let linux_exe_path = fs::read_link(proc_path.join("exe")).ok().map(|p| {
            let s = p.to_string_lossy().into_owned();
            s.strip_suffix(" (deleted)").unwrap_or(&s).to_string()
        });

        let (name, exe_path, is_proton_exe) =
            if let Some((win_name, win_path)) = extract_wine_exe_from_cmdline(&cmdline_bytes) {
                (win_name, Some(win_path), true)
            } else if let Some(ref p) = linux_exe_path {
                let base = path_basename(p).to_string();
                let final_name = if base.is_empty() { comm.clone() } else { base };
                (final_name, linux_exe_path.clone(), false)
            } else {
                (comm.clone(), None, false)
            };

        if name.is_empty() {
            continue;
        }

        let is_steam_game = !is_proton_exe && linux_exe_path.is_some() && has_steam_app_env(pid);
        let is_system = is_linux_system_process(
            &name,
            exe_path.as_deref(),
            is_proton_exe,
            cmdline_bytes.is_empty(),
            is_steam_game,
        );

        // Tag Proton / Wine and Steam games with a clean title so Steam Deck & Linux users
        // immediately spot their running game at the top of the Process Picker.
        let window_title = if is_proton_exe && !is_system {
            let stem = name
                .strip_suffix(".exe")
                .or_else(|| name.strip_suffix(".EXE"))
                .unwrap_or(&name);
            Some(format!("{stem} (Proton)"))
        } else if is_steam_game && !is_system {
            Some(format!("{name} (Steam)"))
        } else {
            None
        };

        results.push(ProcessInfo {
            pid,
            name,
            exe_path,
            window_title,
            is_system,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_maps_with_spaces_and_proton_modules() {
        let sample = "\
00400000-00401000 r--p 00000000 08:02 1001 /home/deck/.local/share/Steam/steamapps/common/My Game/Game.exe
00401000-00500000 r-xp 00001000 08:02 1001 /home/deck/.local/share/Steam/steamapps/common/My Game/Game.exe
00500000-00520000 rw-p 00100000 08:02 1001 /home/deck/.local/share/Steam/steamapps/common/My Game/Game.exe
7f0000000000-7f0000010000 rw-p 00000000 00:00 0 [heap]
7f0000020000-7f0000030000 rw-p 00000000 00:00 0
7ffe00000000-7ffe00002000 r--p 00000000 00:00 0 [vvar]
7ffe00010000-7ffe00020000 rw-p 00000000 00:00 0 /dev/dri/renderD128
";
        let regions = parse_proc_maps(sample);
        assert_eq!(regions.len(), 7);
        assert!(regions[0].is_scannable());
        assert!(!regions[5].is_scannable()); // [vvar]
        assert!(!regions[6].is_scannable()); // /dev/dri/...

        let mods = modules_from_regions(&regions);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "Game.exe");
        assert_eq!(mods[0].base_address, 0x0040_0000);
        assert_eq!(mods[0].size, 0x0012_0000);
    }

    #[test]
    fn extracts_proton_exe_from_cmdline() {
        let cmd = b"Z:\\home\\deck\\.local\\share\\Steam\\steamapps\\common\\Hades\\Hades.exe\0-fullscreen\0";
        let (name, full) = extract_wine_exe_from_cmdline(cmd).unwrap();
        assert_eq!(name, "Hades.exe");
        assert!(full.ends_with("Hades.exe"));
    }
}
