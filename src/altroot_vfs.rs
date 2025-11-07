use std::io::{Read, Write};
use std::path::PathBuf;
use vfs::{AltrootFS, PhysicalFS, VfsError, VfsPath, VfsResult, error::VfsErrorKind};

pub fn create_and_test_altroot_fs() -> VfsResult<()> {
    // 1. Create a temporary directory for the underlying physical filesystem
    let temp_dir = PathBuf::from("/tmp/vfs_altroot_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // 2. Create a PhysicalFS instance pointing to the temporary directory
    let physical_fs: VfsPath = PhysicalFS::new(temp_dir.clone()).into();

    // Create some files and directories within the physical filesystem
    physical_fs.join("data")?.create_dir()?;
    physical_fs
        .join("data/file1.txt")?
        .create_file()? 
        .write_all(b"Content of file1")?;
    physical_fs.join("data/subdir")?.create_dir()?;
    physical_fs
        .join("data/subdir/file2.txt")?
        .create_file()? 
        .write_all(b"Content of file2")?;
    physical_fs
        .join("other_file.txt")?
        .create_file()? 
        .write_all(b"This file is outside altroot")?;

    // 3. Create an AltrootFS instance, with its root set to "/data" within the physical_fs
    let altroot_path_in_physical_fs = physical_fs.join("data")?;
    let altroot_fs_instance = AltrootFS::new(altroot_path_in_physical_fs);
    let altroot_fs: VfsPath = altroot_fs_instance.into();

    // Access file1.txt through AltrootFS
    let file1_altroot = altroot_fs.join("file1.txt")?;
    assert!(file1_altroot.exists()?);
    let mut content = String::new();
    file1_altroot.open_file()?.read_to_string(&mut content)?;
    assert_eq!(content, "Content of file1");

    // Access file2.txt through AltrootFS
    let file2_altroot = altroot_fs.join("subdir/file2.txt")?;
    assert!(file2_altroot.exists()?);
    content.clear();
    file2_altroot.open_file()?.read_to_string(&mut content)?;
    assert_eq!(content, "Content of file2");

    // Attempt to access a file outside the AltrootFS's defined root
    let outside_file = altroot_fs.join("../other_file.txt");

    match outside_file {
        Ok(path) => {
            if path.exists()? {
                panic!("Unexpectedly found: {:?}", path);
            } else {
                // Correctly did not find /other_file.txt (via AltrootFS, it's outside its root).
            }
        }
        Err(e) if matches!(*e.kind(), VfsErrorKind::FileNotFound) => {
            // Correctly did not find /other_file.txt (via AltrootFS, it's outside its root).
        }
        Err(e) => {
            panic!("Received unexpected error: {:?}", e);
        }
    }

    // Clean up the temporary directory
    std::fs::remove_dir_all(&temp_dir)?;

    Ok(())
}