use std::io::{Read, Write};
use vfs::{MemoryFS, VfsPath, VfsResult};

pub fn create_and_test_memory_fs() -> VfsResult<()> {
    // Initialize an in-memory file system
    let fs = MemoryFS::new();
    let root: VfsPath = fs.into();

    // --- Create a directory ---
    let data_dir = root.join("data")?;
    data_dir.create_dir_all()?;
    assert!(data_dir.exists()?);

    // --- Write to a file in the directory ---
    let config_path = data_dir.join("config.txt")?;
    let config_content = "{\"setting\": \"value\", \"enabled\": true}";

    // Open file for writing (create if it doesn't exist)
    let mut file = config_path.create_file()?;
    file.write_all(config_content.as_bytes())?;
    file.flush()?;

    // --- Read from the file ---
    let mut read_content = String::new();
    // Open file for reading
    let mut read_file = config_path.open_file()?;
    read_file.read_to_string(&mut read_content)?;

    assert_eq!(read_content, config_content);

    // --- Create another file in the root ---
    let readme_path = root.join("README.md")?;
    let readme_content =
        "# GitVFS MemoryFS Example\nThis demonstrates in-memory file system operations.";
    let mut readme_file = readme_path.create_file()?;
    readme_file.write_all(readme_content.as_bytes())?;
    readme_file.flush()?;

    Ok(())
}
