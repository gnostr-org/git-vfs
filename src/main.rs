mod cli;

use clap::Parser;
use git_vfs::{GitVfs, GitObjectKind, Commit};
use std::collections::HashMap;

fn main() {
    let args = cli::Args::parse();
    let mut vfs = GitVfs::new();

    match args.command {
        cli::Commands::Init => {
            // In a real Git VFS, 'init' would set up the repository structure.
            // For this in-memory version, creating a new GitVfs instance is sufficient.
            println!("Initialized empty Git VFS repository.");
        }
        cli::Commands::Commit { message } => {
            // Create a dummy blob and tree for the commit
            let dummy_blob_data = b"Initial content for commit";
            let dummy_blob_hash = vfs.create_blob(dummy_blob_data).expect("Failed to create dummy blob");

            let mut dummy_tree_entries = HashMap::new();
            dummy_tree_entries.insert("dummy_file.txt".to_string(), (dummy_blob_hash.clone(), GitObjectKind::Blob));
            let dummy_tree_hash = vfs.create_tree(dummy_tree_entries).expect("Failed to create dummy tree");

            // Create the commit
            let commit_hash = vfs.create_commit(
                "CLI User", // Author
                &message,
                &dummy_tree_hash,
                vec![], // No parents for the first commit
            ).expect("Failed to create commit");

            // Set the ref and HEAD to the new commit
            vfs.create_ref("refs/heads/main", &commit_hash).expect("Failed to create ref");
            vfs.set_head("refs/heads/main").expect("Failed to set HEAD");

            println!("Created commit: {} with message: '{}'", commit_hash, message);
        }
        cli::Commands::Log => {
            let head_ref = vfs.get_head().expect("No HEAD set. Initialize a repository or make a commit first.");
            let head_hash = vfs.get_ref(&head_ref).expect("HEAD does not point to a valid ref.");

            println!("Commit history for {}:", head_ref);
            match vfs.walk_history(&head_hash) {
                Ok(history) => {
                    if history.is_empty() {
                        println!("  (No commits found)");
                    } else {
                        for commit_hash in history {
                            match vfs.get_object(&commit_hash) {
                                Ok(git_vfs::GitObject::Commit(commit)) => {
                                    println!("  commit: {}", commit_hash);
                                    println!("  Author: {}", commit.author);
                                    println!("  Message: {}", commit.message);
                                    println!("  Parents: {:?}", commit.parent_hashes);
                                    println!(); // Add a blank line for readability
                                }
                                Ok(_) => println!("  commit: {} (unexpected object type)", commit_hash), // Should not happen if walk_history is correct
                                Err(e) => println!("  commit: {} (error retrieving: {:?})", commit_hash, e),
                            }
                        }
                    }
                }
                Err(e) => println!("Error walking history: {:?}", e),
            }
        }
    }
}