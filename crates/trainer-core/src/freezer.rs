use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::memory::{MemoryValue, PointerWidth};
use crate::process::ProcessHandle;

/// How a frozen entry locates its target address on each tick.
#[derive(Debug, Clone)]
pub enum FreezeTarget {
    /// A fixed memory address.
    Absolute(usize),
    /// A base address + pointer chain re-resolved on each tick.
    PointerChain {
        base_address: usize,
        offsets: Vec<isize>,
        width: PointerWidth,
    },
    /// A module-relative pointer chain re-resolved on each tick.
    ModulePointerChain {
        module: String,
        base_offset: usize,
        offsets: Vec<isize>,
        width: PointerWidth,
    },
}

#[derive(Debug, Clone)]
struct FreezeEntry {
    target: FreezeTarget,
    value: MemoryValue,
}

/// Manages a background thread that periodically re-writes frozen memory values.
pub struct FreezeManager {
    entries: Arc<Mutex<HashMap<String, FreezeEntry>>>,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl FreezeManager {
    /// Starts a new freeze loop operating on `process` every `interval` (e.g. 50ms).
    pub fn start(process: Arc<ProcessHandle>, interval: Duration) -> Self {
        let entries: Arc<Mutex<HashMap<String, FreezeEntry>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let running = Arc::new(AtomicBool::new(true));

        let thread_entries = Arc::clone(&entries);
        let thread_running = Arc::clone(&running);

        let worker = thread::spawn(move || {
            while thread_running.load(Ordering::Relaxed) {
                if !process.is_alive() {
                    break;
                }

                let snapshot: Vec<FreezeEntry> = {
                    let guard = thread_entries.lock().unwrap();
                    guard.values().cloned().collect()
                };

                for entry in snapshot {
                    let resolved = match &entry.target {
                        FreezeTarget::Absolute(addr) => Ok(*addr),
                        FreezeTarget::PointerChain {
                            base_address,
                            offsets,
                            width,
                        } => process.resolve_pointer_chain(*base_address, offsets, *width),
                        FreezeTarget::ModulePointerChain {
                            module,
                            base_offset,
                            offsets,
                            width,
                        } => process.resolve_module_pointer_chain(
                            module,
                            *base_offset,
                            offsets,
                            *width,
                        ),
                    };

                    if let Ok(addr) = resolved {
                        let _ = process.write_value(addr, &entry.value);
                    }
                }

                thread::sleep(interval);
            }
        });

        Self {
            entries,
            running,
            worker: Some(worker),
        }
    }

    pub fn freeze(&self, id: impl Into<String>, target: FreezeTarget, value: MemoryValue) {
        let mut guard = self.entries.lock().unwrap();
        guard.insert(id.into(), FreezeEntry { target, value });
    }

    pub fn unfreeze(&self, id: &str) -> bool {
        let mut guard = self.entries.lock().unwrap();
        guard.remove(id).is_some()
    }

    pub fn unfreeze_all(&self) {
        let mut guard = self.entries.lock().unwrap();
        guard.clear();
    }

    pub fn is_frozen(&self, id: &str) -> bool {
        let guard = self.entries.lock().unwrap();
        guard.contains_key(id)
    }

    pub fn active_count(&self) -> usize {
        let guard = self.entries.lock().unwrap();
        guard.len()
    }
}

impl Drop for FreezeManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}
