// =================================================================
// FILE: src/memory_vfs.rs
// ACTION: Full example implementing continuous memory polling using TiKV Jemalloc crates.
// =================================================================

use std::io::{Read, Write};
use vfs::{MemoryFS, VfsPath, VfsResult};
// Required for polling:
use std::thread;
use std::time::Duration; 

// --- CONDITIONAL ALLOCATOR SETUP ---
// Uses the highly compatible `tikv-jemallocator` for the global allocator.
#[cfg(feature = "memory_profiling")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;


// --- MEMORY INTROSPECTION FUNCTION ---
// This function reads and prints the current memory statistics using tikv-jemalloc-ctl.
#[cfg(feature = "memory_profiling")]
pub fn report_current_memory() -> Result<(), Box<dyn std::error::Error>> {
    
    // Define the error mapping closure once for the stats read calls.
    let error_mapper = |e| -> Box<dyn std::error::Error> { format!("tikv-jemalloc-ctl stats error: {}", e).into() };
    
    // CRUCIAL: Advance the epoch to force cached allocator statistics to update.
    // FIX E0283: Explicitly cast the closure return type to resolve type ambiguity.
    tikv_jemalloc_ctl::epoch::advance()
        .map_err(|e| -> Box<dyn std::error::Error> { format!("tikv-jemalloc-ctl epoch advance error: {}", e).into() })?;
    
    // Read statistics 
    let resident = tikv_jemalloc_ctl::stats::resident::read()
        .map_err(error_mapper)?;
    let allocated = tikv_jemalloc_ctl::stats::allocated::read()
        .map_err(error_mapper)?;
    let active = tikv_jemalloc_ctl::stats::active::read()
        .map_err(error_mapper)?;
    
    // Print the report
    println!("[MEM REPORT] RSS: {} bytes, Allocated: {} bytes, Active: {} bytes", 
             resident, allocated, active);
    
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
                Ok(_) => {},
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
