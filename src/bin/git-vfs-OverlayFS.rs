// src/bin/git-vfs-OverlayFS.rs

use vfs::{MemoryFS, OverlayFS, VfsPath, VfsResult};
use std::io::{Read, Write};
use std::ffi::OsStr;

fn main() -> VfsResult<()> {
    // 1. Create a lower (read-only) filesystem
    let lower_fs = MemoryFS::new();
    let lower_root: VfsPath = lower_fs.into();

    lower_root.join("file_in_lower.txt")?.create_file()?.write_all(b"Content from lower")?;
    lower_root.join("common_file.txt")?.create_file()?.write_all(b"Common content from lower")?;
    lower_root.join("lower_dir")?.create_dir()?;
    lower_root.join("lower_dir/lower_file.txt")?.create_file()?.write_all(b"File in lower_dir")?;

    println!("--- Lower Filesystem Content ---");
    // Corrected directory listing: iterate directly over the result of read_dir()
    // and use filename() to get the name from VfsPath.
    if let Ok(entries) = lower_root.read_dir() {
        for entry in entries {
            println!("{}", entry.filename());
        }
    } else {
        println!("   Could not list directory contents.");
    }
    println!("--------------------------------\n");

    // 2. Create an upper (read/write) filesystem
    let upper_fs = MemoryFS::new();
    let upper_root: VfsPath = upper_fs.into();

    upper_root.join("file_in_upper.txt")?.create_file()?.write_all(b"Content from upper")?;
    upper_root.join("common_file.txt")?.create_file()?.write_all(b"Common content from upper (shadows lower)")?;
    upper_root.join("upper_dir")?.create_dir()?;
    upper_root.join("upper_dir/upper_file.txt")?.create_file()?.write_all(b"File in upper_dir")?;

    println!("--- Upper Filesystem Content ---");
    if let Ok(entries) = upper_root.read_dir() {
        for entry in entries {
            if let Some(filename_osstr) = Some(entry.filename()) {
                println!("{}", filename_osstr.to_string());
            } else {
                println!("   (entry with no filename)");
            }
        }
    } else {
        println!("   Could not list directory contents.");
    }
    println!("--------------------------------\n");

    // 3. Create the OverlayFS
    // Corrected OverlayFS::new signature: it takes a slice of VfsPaths.
    // The first path in the slice is the upper (writable) layer.
    let overlay_fs = OverlayFS::new(&[upper_root.clone(), lower_root.clone()]);
    let overlay_root: VfsPath = overlay_fs.into();

    println!("--- OverlayFS Content (Initial) ---");
    if let Ok(entries) = overlay_root.read_dir() {
        for entry in entries {
            if let Some(filename_osstr) = Some(entry.filename()) {
                println!("{}", filename_osstr.to_string());
            } else {
                println!("   (entry with no filename)");
            }
        }
    } else {
        println!("   Could not list directory contents.");
    }
    println!("-----------------------------------\n");

    // 4. Demonstrate file access and shadowing
    let mut buffer = String::new();

    // Read a file only in the lower layer
    overlay_root.join("file_in_lower.txt")?.open_file()?.read_to_string(&mut buffer)?;
    println!("Content of 'file_in_lower.txt': '{}'", buffer); // Should be from lower
    buffer.clear();

    // Read a file only in the upper layer
    overlay_root.join("file_in_upper.txt")?.open_file()?.read_to_string(&mut buffer)?;
    println!("Content of 'file_in_upper.txt': '{}'", buffer); // Should be from upper
    buffer.clear();

    // Read a file present in both (upper shadows lower)
    overlay_root.join("common_file.txt")?.open_file()?.read_to_string(&mut buffer)?;
    println!("Content of 'common_file.txt': '{}'", buffer); // Should be from upper
    buffer.clear();

    // Accessing merged directories
    println!("\n--- Content of 'lower_dir' (merged) ---");
    if let Ok(entries) = overlay_root.join("lower_dir")?.read_dir() {
        for entry in entries {
            if let Some(filename_osstr) = Some(entry.filename()) {
                println!("{}", filename_osstr.to_string());
            } else {
                println!("   (entry with no filename)");
            }
        }
    } else {
        println!("   Could not list directory contents for lower_dir.");
    }
    println!("---------------------------------------\n");

    // 5. Demonstrate writing to the overlay (modifies the upper layer)
    let new_file_path = overlay_root.join("new_file_on_overlay.txt")?;
    new_file_path.create_file()?.write_all(b"This is a new file created on the overlay")?;

    let modified_common_file_path = overlay_root.join("common_file.txt")?;
    modified_common_file_path.create_file()?.write_all(b"Modified content on overlay")?; // This writes to the upper layer

    println!("--- OverlayFS Content (After write operations) ---");
    if let Ok(entries) = overlay_root.read_dir() {
        for entry in entries {
            if let Some(filename_osstr) = Some(entry.filename()) {
                println!("{}", filename_osstr.to_string());
            } else {
                println!("   (entry with no filename)");
            }
        }
    } else {
        println!("   Could not list directory contents.");
    }
    println!("--------------------------------------------------\n");

    // Verify the new file and modified file are in the upper layer
    println!("--- Upper Filesystem Content (After overlay writes) ---");
    if let Ok(entries) = upper_root.read_dir() {
        for entry in entries {
            if let Some(filename_osstr) = Some(entry.filename()) {
                println!("{}", filename_osstr.to_string());
            } else {
                println!("   (entry with no filename)");
            }
        }
    } else {
        println!("   Could not list directory contents.");
    }
    println!("-----------------------------------------------------\n");

    // Verify the lower layer remains unchanged
    println!("--- Lower Filesystem Content (Unchanged) ---");
    if let Ok(entries) = lower_root.read_dir() {
        for entry in entries {
            if let Some(filename_osstr) = Some(entry.filename()) {
                println!("{}", filename_osstr.to_string());
            } else {
                println!("   (entry with no filename)");
            }
        }
    } else {
        println!("   Could not list directory contents.");
    }
    println!("--------------------------------------------\n");

    Ok(())
}
