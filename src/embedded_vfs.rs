use rust_embed::RustEmbed;
use vfs::{EmbeddedFS, VfsPath, VfsResult};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[derive(RustEmbed, Debug)]
#[folder = "$CARGO_MANIFEST_DIR/src/empty/.git"]
pub struct Asset;

pub fn extract_embedded_git_to_temp_dir() -> VfsResult<TempDir> {
    let temp_dir = TempDir::new().map_err(|e| e.into())?;
    let embedded_fs = EmbeddedFS::<Asset>::new();
    let root: VfsPath = embedded_fs.into();

    fn copy_recursively(src: &VfsPath, dest: &Path) -> VfsResult<()> {
        if src.is_dir()? {
            fs::create_dir_all(dest).map_err(|e| e.into())?;
            for entry in src.read_dir()? {
                copy_recursively(&entry, &dest.join(entry.filename()))?;
            }
        } else if src.is_file()? {
            let mut file = src.open_file()?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            fs::write(dest, &buffer).map_err(|e| e.into())?;
        }
        Ok(())
    }

    copy_recursively(&root, temp_dir.path())?;
    Ok(temp_dir)
}

pub fn create_and_test_embedded_fs() -> VfsResult<()> {
    println!("-- Embedded .git FS Example ---");

    let embedded_fs = EmbeddedFS::<Asset>::new();
    let root: VfsPath = embedded_fs.into();

    println!("\n--- Listing Root Directory (.git) ---");
    if let Ok(entries) = root.read_dir() {
        for entry in entries {
            println!("{}", entry.filename());
        }
    } else {
        println!("   Could not list root directory contents.");
    }

    println!("\n--- Reading HEAD ---");
    let mut buffer = String::new();
    match root.join("HEAD")?.open_file() {
        Ok(mut file) => {
            file.read_to_string(&mut buffer)?;
            println!("Content of 'HEAD': '{}'", buffer);
        }
        Err(e) => println!("Error reading HEAD: {}", e),
    }
    buffer.clear();

    println!("\n--- Listing refs Directory ---");
    match root.join("refs") {
        Ok(refs_dir) => {
            if let Ok(entries) = refs_dir.read_dir() {
                for entry in entries {
                    println!("{}", entry.filename());
                }
            }
        }
        Err(e) => println!("Error joining path to refs directory: {}", e),
    }

    println!("\n--- Reading config ---");
    match root.join("config")?.open_file() {
        Ok(mut file) => {
            file.read_to_string(&mut buffer)?;
            println!("Content of 'config': '{}'", buffer);
        }
        Err(e) => println!("Error reading config: {}", e),
    }

    Ok(())
}
