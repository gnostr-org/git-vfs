use std::io::{Read, Write};
use vfs::{MemoryFS, OverlayFS, VfsPath, VfsResult};

pub fn create_and_test_overlay_fs() -> VfsResult<()> {
    // 1. Create a lower (read-only) filesystem
    let lower_fs = MemoryFS::new();
    let lower_root: VfsPath = lower_fs.into();

    lower_root
        .join("file_in_lower.txt")?
        .create_file()?
        .write_all(b"Content from lower")?;
    lower_root
        .join("common_file.txt")?
        .create_file()?
        .write_all(b"Common content from lower")?;
    lower_root.join("lower_dir")?.create_dir()?;
    lower_root
        .join("lower_dir/lower_file.txt")?
        .create_file()?
        .write_all(b"File in lower_dir")?;

    // 2. Create an upper (read/write) filesystem
    let upper_fs = MemoryFS::new();
    let upper_root: VfsPath = upper_fs.into();

    upper_root
        .join("file_in_upper.txt")?
        .create_file()?
        .write_all(b"Content from upper")?;
    upper_root
        .join("common_file.txt")?
        .create_file()?
        .write_all(b"Common content from upper (shadows lower)")?;
    upper_root.join("upper_dir")?.create_dir()?;
    upper_root
        .join("upper_dir/upper_file.txt")?
        .create_file()?
        .write_all(b"File in upper_dir")?;

    // 3. Create the OverlayFS
    let overlay_fs = OverlayFS::new(&[upper_root.clone(), lower_root.clone()]);
    let overlay_root: VfsPath = overlay_fs.into();

    // 4. Demonstrate file access and shadowing
    let mut buffer = String::new();

    // Read a file only in the lower layer
    overlay_root
        .join("file_in_lower.txt")?
        .open_file()?
        .read_to_string(&mut buffer)?;
    assert_eq!(buffer, "Content from lower");
    buffer.clear();

    // Read a file only in the upper layer
    overlay_root
        .join("file_in_upper.txt")?
        .open_file()?
        .read_to_string(&mut buffer)?;
    assert_eq!(buffer, "Content from upper");
    buffer.clear();

    // Read a file present in both (upper shadows lower)
    overlay_root
        .join("common_file.txt")?
        .open_file()?
        .read_to_string(&mut buffer)?;
    assert_eq!(buffer, "Common content from upper (shadows lower)");
    buffer.clear();

    // 5. Demonstrate writing to the overlay (modifies the upper layer)
    let new_file_path = overlay_root.join("new_file_on_overlay.txt")?;
    new_file_path
        .create_file()?
        .write_all(b"This is a new file created on the overlay")?;

    let modified_common_file_path = overlay_root.join("common_file.txt")?;
    modified_common_file_path
        .create_file()?
        .write_all(b"Modified content on overlay")?;

    // Verify the new file and modified file are in the upper layer
    let mut upper_entries = Vec::new();
    for entry in upper_root.read_dir()? {
        upper_entries.push(entry.filename().to_string());
    }
    assert!(upper_entries.contains(&"new_file_on_overlay.txt".to_string()));
    assert!(upper_entries.contains(&"common_file.txt".to_string()));

    let mut buffer_new = String::new();
    upper_root
        .join("new_file_on_overlay.txt")?
        .open_file()?
        .read_to_string(&mut buffer_new)?;
    assert_eq!(buffer_new, "This is a new file created on the overlay");
    buffer_new.clear();

    upper_root
        .join("common_file.txt")?
        .open_file()?
        .read_to_string(&mut buffer_new)?;
    assert_eq!(buffer_new, "Modified content on overlay");

    // Verify the lower layer remains unchanged
    let mut lower_entries = Vec::new();
    for entry in lower_root.read_dir()? {
        lower_entries.push(entry.filename().to_string());
    }
    assert!(lower_entries.contains(&"file_in_lower.txt".to_string()));
    assert!(lower_entries.contains(&"common_file.txt".to_string()));
    assert!(!lower_entries.contains(&"new_file_on_overlay.txt".to_string()));

    let mut buffer_lower = String::new();
    lower_root
        .join("common_file.txt")?
        .open_file()?
        .read_to_string(&mut buffer_lower)?;
    assert_eq!(buffer_lower, "Common content from lower");

    Ok(())
}
