use rust_embed::RustEmbed;
use vfs::{EmbeddedFS, VfsPath, VfsResult};

#[derive(RustEmbed, Debug)]
#[folder = "$CARGO_MANIFEST_DIR/embedded_files"]
struct Asset;

fn main() -> VfsResult<()> {
    println!("--- EmbeddedFS Example ---");

    let embedded_fs = EmbeddedFS::<Asset>::new();
    let root: VfsPath = embedded_fs.into();

    println!("\n--- Listing Root Directory ---");
    if let Ok(entries) = root.read_dir() {
        for entry in entries {
            println!("{}", entry.filename());
        }
    } else {
        println!("   Could not list root directory contents.");
    }

    println!("\n--- Reading hello.txt ---");
    let mut buffer = String::new();
    match root.join("hello.txt")?.open_file() {
        Ok(mut file) => {
            file.read_to_string(&mut buffer)?;
            println!("Content of 'hello.txt': '{}'", buffer);
        }
        Err(e) => println!("Error reading hello.txt: {}", e),
    }
    buffer.clear();

    println!("\n--- Listing data Directory ---");
    match root.join("data") {
        Ok(data_dir) => {
            if let Ok(entries) = data_dir.read_dir() {
                for entry in entries {
                    println!("{}", entry.filename());
                }
            } else {
                println!("   Could not list data directory contents.");
            }
        }
        Err(e) => println!("Error joining path to data directory: {}", e),
    }

    println!("\n--- Reading data/config.json ---");
    match root.join("data/config.json")?.open_file() {
        Ok(mut file) => {
            file.read_to_string(&mut buffer)?;
            println!("Content of 'data/config.json': '{}'", buffer);
        }
        Err(e) => println!("Error reading data/config.json: {}", e),
    }

    Ok(())
}
