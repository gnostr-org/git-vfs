use vfs::{VfsPath, PhysicalFS, VfsError};

fn main() -> Result<(), Box<dyn std::error::Error>>{
let root: VfsPath = PhysicalFS::new(std::env::current_dir().unwrap()).into();
assert!(root.exists()?);

let mut content = String::new();
root.join("README.md")?.open_file()?.read_to_string(&mut content)?;
assert!(content.contains("vfs"));
Ok(())
}
