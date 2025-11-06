#![allow(unused)]

use clap::{Parser, Subcommand};
use git_vfs::{GitVfs, GitObjectKind, Commit};
use std::collections::HashMap;

#[derive(Parser, Debug)]
#[command(author, version, about = "A distributed Git Virtual File System", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new Git VFS repository
    Init,
    /// Create a new commit
    Commit {
        /// The commit message
        #[arg(short, long)]
        message: String,
    },
    /// Show commit history
    Log,
    // Add other commands here as needed
}

// Note: The actual GitVfs operations will be handled in src/main.rs
// This file primarily defines the CLI structure.
