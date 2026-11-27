#![cfg(any(windows, target_os = "linux"))]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use trainer_core::{
    AobPattern, BytePatch, FreezeManager, FreezeTarget, MemoryScanner, MemoryValue, PointerWidth,
    ProcessHandle, ScanFilter, ScanOptions, ValueType,
};

struct RunningTarget {
    child: Child,
    meta: HashMap<String, String>,
}

impl RunningTarget {
    fn spawn() -> Self {
        // Locate dummy-target binary next to the test binary in target/debug/
        let exe_name = format!("dummy-target{}", std::env::consts::EXE_SUFFIX);
        let mut exe = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        exe.pop(); // crates/
        exe.pop(); // freeplay/
        exe.push("target");
        exe.push("debug");
        exe.push(&exe_name);

        assert!(
            exe.exists(),
            "{exe_name} not found at {:?}; run `cargo build --workspace` before running tests",
            exe
        );

        let mut child = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn dummy-target");

        let stdout = child.stdout.take().expect("stdout piped");
        let reader = BufReader::new(stdout);
        let mut meta = HashMap::new();

        for line in reader.lines() {
            let line = line.expect("failed to read dummy-target stdout");
            if line.trim() == "READY" {
                break;
            }
            if let Some((k, v)) = line.split_once('=') {
                meta.insert(k.trim().to_string(), v.trim().to_string());
            }
        }

        Self { child, meta }
    }

    fn hex(&self, key: &str) -> usize {
        let raw = self.meta.get(key).unwrap_or_else(|| panic!("missing {key}"));
        let trimmed = raw.strip_prefix("0x").unwrap_or(raw);
        usize::from_str_radix(trimmed, 16).expect("valid hex")
    }

    fn pid(&self) -> u32 {
        self.meta.get("PID").expect("PID").parse().expect("u32 pid")
    }
}

impl Drop for RunningTarget {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn end_to_end_memory_engine_against_dummy_target() {
    let target = RunningTarget::spawn();
    let proc_handle = Arc::new(ProcessHandle::attach_pid(target.pid()).expect("attach_pid"));
    assert!(proc_handle.is_alive());

    // 1. Module enumeration
    let target_module = format!("dummy-target{}", std::env::consts::EXE_SUFFIX);
    let base = proc_handle
        .module_base(&target_module)
        .expect("module_base");
    assert_ne!(base, 0);

    // 2. Direct read & write (gold & stamina)
    let gold_addr = target.hex("GOLD_ADDR");
    let stamina_addr = target.hex("STAMINA_ADDR");
    assert_eq!(
        proc_handle.read_value(gold_addr, ValueType::I32).unwrap(),
        MemoryValue::I32(250)
    );
    proc_handle
        .write_value(gold_addr, &MemoryValue::I32(99_999))
        .unwrap();
    assert_eq!(
        proc_handle.read_value(gold_addr, ValueType::I32).unwrap(),
        MemoryValue::I32(99_999)
    );

    proc_handle
        .write_value(stamina_addr, &MemoryValue::F32(100.0))
        .unwrap();
    assert_eq!(
        proc_handle
            .read_value(stamina_addr, ValueType::F32)
            .unwrap(),
        MemoryValue::F32(100.0)
    );

    // 3. Multi-level pointer chain resolution: base_addr -> +STATS_PTR_OFFSET -> +GOLD_OFFSET
    let base_addr = target.hex("BASE_ADDR");
    let stats_ptr_off = target.hex("STATS_PTR_OFFSET") as isize;
    let gold_off = target.hex("GOLD_OFFSET") as isize;
    let resolved_gold = proc_handle
        .resolve_pointer_chain(base_addr, &[stats_ptr_off, gold_off], PointerWidth::Bits64)
        .expect("resolve_pointer_chain");
    assert_eq!(resolved_gold, gold_addr);

    // 4. AoB signature scan with wildcards
    let sig_addr = target.hex("SIG_ADDR");
    let pattern = AobPattern::parse("54 52 4E 52 13 37 ?? DE AA BB ?? DD 01 02 03 04").unwrap();
    let hits = proc_handle
        .scan_aob(
            &pattern,
            ScanOptions {
                executable_only: false,
                start_address: Some(sig_addr.saturating_sub(0x1000)),
                end_address: Some(sig_addr.saturating_add(0x1000)),
                max_results: Some(4),
            },
        )
        .expect("scan_aob");
    assert!(
        hits.contains(&sig_addr),
        "expected AoB scan to find {sig_addr:#x}, got {hits:x?}"
    );

    // 5. Reversible BytePatch
    let mut patch = BytePatch::new(sig_addr, vec![0x90, 0x90, 0x90, 0x90]);
    patch.apply(&proc_handle).expect("apply patch");
    assert!(patch.is_applied());
    assert_eq!(
        proc_handle.read_bytes(sig_addr, 4).unwrap(),
        vec![0x90, 0x90, 0x90, 0x90]
    );
    patch.revert(&proc_handle).expect("revert patch");
    assert!(!patch.is_applied());
    assert_eq!(
        proc_handle.read_bytes(sig_addr, 4).unwrap(),
        vec![0x54, 0x52, 0x4E, 0x52]
    );

    // 6. Background FreezeManager (holds health at 500 while dummy-target decrements it every 25ms)
    let health_off = target.hex("HEALTH_OFFSET") as isize;
    let health_addr = target.hex("HEALTH_ADDR");
    let freezer = FreezeManager::start(Arc::clone(&proc_handle), Duration::from_millis(10));
    freezer.freeze(
        "god_mode_hp",
        FreezeTarget::PointerChain {
            base_address: base_addr,
            offsets: vec![stats_ptr_off, health_off],
            width: PointerWidth::Bits64,
        },
        MemoryValue::I32(500),
    );
    assert!(freezer.is_frozen("god_mode_hp"));

    thread::sleep(Duration::from_millis(150));
    let frozen_hp = match proc_handle.read_value(health_addr, ValueType::I32).unwrap() {
        MemoryValue::I32(v) => v,
        other => panic!("unexpected value: {other:?}"),
    };
    // Because dummy-target decrements by 1 on its 25ms tick and freezer rewrites 500 every 10ms,
    // health should stay right at 499..=500 instead of decaying toward 0.
    assert!(
        (498..=500).contains(&frozen_hp),
        "expected frozen health near 500, got {frozen_hp}"
    );

    assert!(freezer.unfreeze("god_mode_hp"));
    assert_eq!(freezer.active_count(), 0);

    // 7. Live MemoryScanner: first_scan(99_999) -> change to 123_456 -> next_scan(123_456)
    let mut scanner = MemoryScanner::new(ValueType::I32);
    let count1 = scanner
        .first_scan(&proc_handle, &MemoryValue::I32(99_999))
        .expect("first_scan");
    assert!(count1 >= 1, "expected at least 1 hit for gold=99_999");

    proc_handle
        .write_value(gold_addr, &MemoryValue::I32(123_456))
        .unwrap();
    let count2 = scanner
        .next_scan(&proc_handle, &ScanFilter::Exact(MemoryValue::I32(123_456)))
        .expect("next_scan");
    assert!(count2 >= 1);
    assert!(
        scanner.hits().iter().any(|h| h.address == gold_addr),
        "expected scanner to isolate gold_addr {gold_addr:#x}"
    );
}
