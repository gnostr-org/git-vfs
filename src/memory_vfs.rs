// =================================================================
// FILE: src/memory_vfs.rs
// ACTION: FIX: Change global allocator and control source to TiKV crates.
// =================================================================

use std::io::{Read, Write};
use vfs::{MemoryFS, VfsPath, VfsResult};

// --- CONDITIONAL ALLOCATOR SETUP ---
// FIXED E0412/E0425: Import Jemalloc from the highly compatible `tikv-jemallocator` crate.
#[cfg(feature = "memory_profiling")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;


// --- MEMORY INTROSPECTION FUNCTION ---
// Now uses `tikv_jemalloc_ctl` for statistics.
#[cfg(feature = "memory_profiling")]
pub fn report_memory_usage() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- MemoryFS Heap Introspection (tikv-jemalloc) ---");

    // Explicit type casting ensures type ambiguity is resolved.
    let error_mapper = |e| -> Box<dyn std::error::Error> { format!("tikv-jemalloc-ctl error: {}", e).into() };

    // Total Resident Set Size (RSS): amount of memory mapped by the allocator.
    let resident = tikv_jemalloc_ctl::stats::resident::read()
        .map_err(error_mapper)?;
    println!("Total Resident Set Size (RSS): {} bytes", resident);

    // Total Allocated Bytes (Active Heap): memory currently allocated by the application.
    let allocated = tikv_jemalloc_ctl::stats::allocated::read()
        .map_err(error_mapper)?;
    println!("Total Allocated Bytes (Active Heap): {} bytes", allocated);

    // Total Active Bytes (Used/Touched): memory in active pages.
    let active = tikv_jemalloc_ctl::stats::active::read()
        .map_err(error_mapper)?;
    println!("Total Active Bytes (Used/Touched): {} bytes", active);

    println!("--------------------------------------------------");
    Ok(())
}

// Dummy function for when profiling is disabled
#[cfg(not(feature = "memory_profiling"))]
pub fn report_memory_usage() -> Result<(), Box<dyn std::error::Error>> {
    println!("\nMemory profiling is not enabled. Compile with --features memory_profiling.");
    Ok(())
}


pub fn create_and_test_memory_fs() -> VfsResult<()> {
    
    let _ = report_memory_usage(); 
    
    let fs = MemoryFS::new();
    let root: VfsPath = fs.into();

    let _ = report_memory_usage(); 

    let data_dir = root.join("data")?;
    data_dir.create_dir_all()?;

    let config_path = data_dir.join("config.txt")?;
    let config_content = "{\"setting\": \"value\", \"enabled\": true}";

    let mut file = config_path.create_file()?;
    file.write_all(config_content.as_bytes())?;
    file.flush()?;

    let _ = report_memory_usage(); 

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

    let _ = report_memory_usage(); 

    Ok(())
}
