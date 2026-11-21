use std::io::{self, Write};
use std::ptr::{read_volatile, write_volatile};
use std::thread;
use std::time::{Duration, Instant};

#[repr(C)]
struct PlayerStats {
    padding: u64,
    health: i32,
    gold: i32,
    stamina: f32,
}

#[repr(C)]
struct PlayerHolder {
    reserved: u64,
    stats_ptr: *mut PlayerStats,
}

/// Safe local target process simulating a single-player game for testing `trainer-core`.
fn main() {
    let stats = Box::new(PlayerStats {
        padding: 0x1122_3344_5566_7788,
        health: 100,
        gold: 250,
        stamina: 75.5,
    });
    let stats_raw = Box::into_raw(stats);

    let holder = Box::new(PlayerHolder {
        reserved: 0xAABB_CCDD_EEFF_0011,
        stats_ptr: stats_raw,
    });
    let holder_raw = Box::into_raw(holder);

    // Pointer-to-pointer base so tests can verify 2-level pointer chain resolution:
    // base_ptr -> (*base_ptr + 8) -> (*stats_ptr + 8 = health, + 12 = gold, + 16 = stamina)
    let mut base_ptr: *mut PlayerHolder = holder_raw;

    // Distinct 16-byte buffer for AoB scan + BytePatch tests
    let mut signature_buf: Box<[u8; 16]> = Box::new([
        0x54, 0x52, 0x4E, 0x52, // "TRNR"
        0x13, 0x37, 0xC0, 0xDE,
        0xAA, 0xBB, 0xCC, 0xDD,
        0x01, 0x02, 0x03, 0x04,
    ]);

    let health_offset = std::mem::offset_of!(PlayerStats, health);
    let gold_offset = std::mem::offset_of!(PlayerStats, gold);
    let stamina_offset = std::mem::offset_of!(PlayerStats, stamina);
    let stats_ptr_offset = std::mem::offset_of!(PlayerHolder, stats_ptr);

    let pid = std::process::id();
    let base_addr = (&mut base_ptr as *mut *mut PlayerHolder) as usize;
    let health_addr = unsafe { &(*stats_raw).health as *const i32 as usize };
    let gold_addr = unsafe { &(*stats_raw).gold as *const i32 as usize };
    let stamina_addr = unsafe { &(*stats_raw).stamina as *const f32 as usize };
    let sig_addr = signature_buf.as_mut_ptr() as usize;

    println!("PID={pid}");
    println!("BASE_ADDR={base_addr:#x}");
    println!("STATS_PTR_OFFSET={stats_ptr_offset:#x}");
    println!("HEALTH_OFFSET={health_offset:#x}");
    println!("GOLD_OFFSET={gold_offset:#x}");
    println!("STAMINA_OFFSET={stamina_offset:#x}");
    println!("HEALTH_ADDR={health_addr:#x}");
    println!("GOLD_ADDR={gold_addr:#x}");
    println!("STAMINA_ADDR={stamina_addr:#x}");
    println!("SIG_ADDR={sig_addr:#x}");
    println!("READY");
    let _ = io::stdout().flush();

    // Run for up to 15 seconds, ticking health down by 1 every 25ms (simulating damage over time).
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        unsafe {
            let hp = read_volatile(&(*stats_raw).health);
            if hp > 0 {
                write_volatile(&mut (*stats_raw).health, hp - 1);
            }
            // Keep signature_buf live so the compiler never optimizes it away.
            let _ = read_volatile(signature_buf.as_ptr());
        }
        thread::sleep(Duration::from_millis(25));
    }

    unsafe {
        let _ = Box::from_raw(holder_raw);
        let _ = Box::from_raw(stats_raw);
    }
}
