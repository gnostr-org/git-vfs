// =================================================================
// FILE: src/memory_vfs.rs
// ACTION: FIX: Converted println! string to a raw string literal (r"...")
//               to resolve the unknown escape character error (\D).
// =================================================================

use std::io::{Read, Write};
use vfs::{MemoryFS, VfsPath, VfsResult};

#[cfg(feature = "memory_profiling")]
use std::sync::Mutex;
#[cfg(feature = "memory_profiling")]
use std::thread; // FIX: Warning on unused import
#[cfg(feature = "memory_profiling")]
use std::time::Duration; // FIX: Warning on unused import // FIX: Warning on unused import

// --- TYPE DEFINITIONS AND GLOBAL STATE ---

/// Stores the previous memory cycle statistics for delta calculation.
#[cfg(feature = "memory_profiling")]
struct MemoryStats {
    resident: usize,
    allocated: usize,
    active: usize,
}

/// Global static variable to hold the memory usage from the LAST cycle.
/// It's wrapped in a Mutex for thread-safe access from the polling thread.
#[cfg(feature = "memory_profiling")]
static LAST_MEMORY_STATS: Mutex<MemoryStats> = Mutex::new(MemoryStats {
    resident: 0,
    allocated: 0,
    active: 0,
});

// --- CONDITIONAL ALLOCATOR SETUP ---
#[cfg(feature = "memory_profiling")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// --- MEMORY INTROSPECTION FUNCTION ---
#[cfg(feature = "memory_profiling")]
pub fn report_current_memory() -> Result<(), Box<dyn std::error::Error>> {
    let error_mapper = |e| -> Box<dyn std::error::Error> {
        format!("tikv-jemalloc-ctl stats error: {}", e).into()
    };

    // 1. Advance Epoch (CRUCIAL for fresh data)
    tikv_jemalloc_ctl::epoch::advance().map_err(|e| -> Box<dyn std::error::Error> {
        format!("tikv-jemalloc-ctl epoch advance error: {}", e).into()
    })?;

    // 2. Read Current Statistics
    let current_resident = tikv_jemalloc_ctl::stats::resident::read().map_err(error_mapper)?;
    let current_allocated = tikv_jemalloc_ctl::stats::allocated::read().map_err(error_mapper)?;
    let current_active = tikv_jemalloc_ctl::stats::active::read().map_err(error_mapper)?;

    // 3. Calculate Delta ($\Delta$)
    let mut last_stats = LAST_MEMORY_STATS.lock().unwrap();

    let delta_resident = current_resident as isize - last_stats.resident as isize;
    let delta_allocated = current_allocated as isize - last_stats.allocated as isize;
    let delta_active = current_active as isize - last_stats.active as isize;

    // 4. Update Last Stats for the next cycle
    last_stats.resident = current_resident;
    last_stats.allocated = current_allocated;
    last_stats.active = current_active;

    // 5. Print the Report including Delta
    // FIXED LINE 74: Converted to raw string literal (r"...")
    if delta_resident != current_resident as isize {
        println!(
            r"[MEM REPORT] RSS: {} ($\Delta$: {:+}) | Allocated: {} ($\Delta$: {:+}) | Active: {} ($\Delta$: {:+})",
            current_resident,
            delta_resident,
            current_allocated,
            delta_allocated,
            current_active,
            delta_active
        );
    } else {
        // FIXED LINE 78: Converted to raw string literal (r"...")
        println!(
            r"[MEM REPORT] RSS: {} | Allocated: {} | Active: {} (Initial Cycle)",
            current_resident, current_allocated, current_active
        );
    }

    Ok(())
}

// Dummy function for when profiling is disabled
#[cfg(not(feature = "memory_profiling"))]
pub fn report_current_memory() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

// --- POLLING THREAD FUNCTION ---
#[cfg(feature = "memory_profiling")]
fn start_memory_polling_thread() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        loop {
            // Report memory usage every 5 seconds
            match report_current_memory() {
                Ok(_) => {}
                Err(e) => eprintln!("Memory polling error: {}", e),
            }
            thread::sleep(Duration::from_secs(5));
        }
    })
}

pub fn create_and_test_memory_fs() -> VfsResult<()> {
    // --- START POLLING ---
    #[cfg(feature = "memory_profiling")]
    let _polling_handle = start_memory_polling_thread();

    // --- EXECUTE VFS OPERATIONS ---
    println!("\nStarting VFS Operations...");

    let fs = MemoryFS::new();
    let root: VfsPath = fs.into();

    let data_dir = root.join("data")?;
    data_dir.create_dir_all()?;
    assert!(data_dir.exists()?);

    let config_path = data_dir.join("config.txt")?;
    let config_content = "{\"setting\": \"value\", \"enabled\": true}";

    let mut file = config_path.create_file()?;
    file.write_all(config_content.as_bytes())?;
    file.flush()?;

    let mut read_content = String::new();
    let mut read_file = config_path.open_file()?;
    read_file.read_to_string(&mut read_content)?;

    assert_eq!(read_content, config_content);

    let readme_path = root.join("README.md")?;
    let readme_content =
        "# GitVFS MemoryFS Example\nThis demonstrates in-memory file system operations.";
    let mut readme_file = readme_path.create_file()?;
    readme_file.write_all(readme_content.as_bytes())?;
    readme_file.flush()?;

    println!("VFS Operations Complete. Memory polling continues in background every 5s.");

    Ok(())
}
