// src/bin/git-vfs-MemoryFS.rs

use vfs::{MemoryFS, VfsPath};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("---\nGitVfs MemoryFS Example ---");

    // Initialize an in-memory file system
    let mut fs = MemoryFS::new();
    let root: VfsPath = fs.into(); // Convert MemoryFS to VfsPath

    // --- Create a directory ---
    println!("\n1. Creating directory 'data'...");
    let data_dir = root.join("data")?;
    data_dir.create_dir_all()?;
    assert!(data_dir.exists()?);
    println!("   Directory 'data' created successfully.");

    // --- Write to a file in the directory ---
    println!("\n2. Writing to 'data/config.txt'...");
    let config_path = data_dir.join("config.txt")?;
    let config_content = "{\"setting\": \"value\", \"enabled\": true}";
    
    // Open file for writing (create if it doesn't exist)
    let mut file = config_path.create_file()?;
    file.write_all(config_content.as_bytes())?;
    println!("   Successfully wrote to 'data/config.txt'.");

    // --- Read from the file ---
    println!("\n3. Reading from 'data/config.txt'...");
    let mut read_content = String::new();
    // Open file for reading
    let mut read_file = config_path.open_file()?;
    read_file.read_to_string(&mut read_content)?;

    println!("   Content read: '{}'", read_content);
    assert_eq!(read_content, config_content);
    println!("   Content verification successful.");

    // --- Create another file in the root ---
    println!("\n4. Creating 'README.md' in root...");
    let readme_path = root.join("README.md")?;
    let readme_content = "# GitVFS MemoryFS Example\nThis demonstrates in-memory file system operations.";
    let mut readme_file = readme_path.create_file()?;
    readme_file.write_all(readme_content.as_bytes())?;
    println!("   Successfully wrote to 'README.md'.");

    // --- List directory contents ---
    // Removed list_dir() call as it's not available.
    // If directory listing is needed, a different approach or crate might be required.
    println!("\n5. Skipping directory listing as list_dir() is not available.");

    println!("\n--- MemoryFS Example Finished ---");
    Ok(())
}