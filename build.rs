use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

// try:
// cargo build --features memory_profiling -j8

fn main() {
    let target_path = Path::new("src/empty");

    // 1. Clean up the target path if it exists as a FILE or a DIRECTORY.
    // We try to remove the path as a file first, and if that fails, we try as a directory.
    if target_path.exists() {
        if target_path.is_file() {
            println!("cargo:warning=Found file at target path. Removing src/empty.");
            if let Err(e) = fs::remove_file(target_path) {
                panic!(
                    "Failed to remove existing file {}: {}",
                    target_path.display(),
                    e
                );
            }
        } else if target_path.is_dir() {
            // If it exists as a directory, we can skip removal, as create_dir_all is idempotent,
            // but if the goal is absolute cleanup, we use remove_dir_all.
            // For simplicity, we let create_dir_all handle the existing directory case.
        }
    }

    // 2. Create the directory `./src/empty` (using create_dir_all for robustness).
    // This function creates all necessary parent directories and succeeds if the directory already exists.
    //println!("cargo:warning=Creating directory: ./src/empty");
    //if let Err(e) = fs::create_dir_all(target_path) {
    //    panic!("Failed to create directory {}: {}", target_path.display(), e);
    //}

    let dir_path = Path::new("src/empty");
    let readme_path = dir_path.join("README.md");

    println!("cargo:rerun-if-changed=build.rs");

    // --- 1. Remove the Directory (if it exists) ---
    if dir_path.exists() {
        match fs::remove_dir_all(dir_path) {
            Ok(_) => println!(
                "Build: Successfully removed directory: {}",
                dir_path.display()
            ),
            Err(e) => {
                panic!(
                    "Build: Failed to remove directory {}: {}",
                    dir_path.display(),
                    e
                );
            }
        }
    } else {
        println!(
            "Build: Directory {} does not exist, skipping removal.",
            dir_path.display()
        );
    }

    // --- 2. Create the Directory ---
    match fs::create_dir_all(dir_path) {
        Ok(_) => println!(
            "Build: Successfully created directory: {}",
            dir_path.display()
        ),
        Err(e) => {
            panic!(
                "Build: Failed to create directory {}: {}",
                dir_path.display(),
                e
            );
        }
    }

    let content = r###"### gnostr-lfs/src/empty

This directory is intentionally kept minimal and serves as a placeholder for the initial
empty tree object in the Git repository history. The first commit creates the project's
root using a known epoch date for historical consistency.

GIT_AUTHOR_NAME=gnostr-vfs

GIT_AUTHOR_EMAIL=admin@gnostr.org

GIT_COMMITTER_NAME=gnostr_dev

GIT_COMMITTER_EMAIL=admin@gnostr.org

GIT_AUTHOR_DATE="Thu, 01 Jan 1970 00:00:00 +0000"

GIT_COMMITTER_DATE="Thu, 01 Jan 1970 00:00:00 +0000"

git commit --allow-empty -m "initial commit"

"###;

    match fs::File::create(&readme_path) {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Ok(_) => println!("Build: Successfully wrote to {}", readme_path.display()),
            Err(e) => panic!(
                "Build: Failed to write content to {}: {}",
                readme_path.display(),
                e
            ),
        },
        Err(e) => panic!(
            "Build: Failed to create file {}: {}",
            readme_path.display(),
            e
        ),
    }

    // --- 3. Run 'git init' inside src/empty ---
    println!(
        "Build: Initializing Git repository in {}",
        dir_path.display()
    );

    let output = Command::new("git")
        .arg("init")
        .current_dir(dir_path) // Crucial: executes 'git init' inside the target folder
        .output()
        .expect("Failed to execute 'git init'");

    if output.status.success() {
        println!("Build: git init successful.");
    } else {
        panic!(
            "Build: git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!("Build: Adding README.md to the index...");

    let output = Command::new("git")
        .arg("add")
        .arg(".") // Use '.' to add all files in the current directory (src/empty)
        .current_dir(dir_path) // Executes 'git add .' inside src/empty
        .output()
        .expect("Failed to execute 'git add'");

    if output.status.success() {
        println!("Build: git add successful.");
    } else {
        panic!(
            "Build: git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Good practice: Rerun build script if the script itself changes.
    println!("cargo:rerun-if-changed=build.rs");
}
