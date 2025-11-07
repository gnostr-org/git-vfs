
// --- IMPORTS ---
use git_vfs::GitVfs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

// --- HELPER FUNCTIONS ---

// Helper to print the state of a GitVfs instance
#[allow(dead_code)]
fn print_vfs_state(vfs: &GitVfs, node_name: &str) {
    println!("--- {} VFS State ---", node_name);
    if let Ok(head) = vfs.get_head() {
        println!("HEAD: {}", head);
        if let Ok(hash) = vfs.get_ref(&head) {
            println!("  -> Ref '{}' points to hash: {}", head, hash);
        } else {
            println!("  -> Ref '{}' not found or invalid.", head);
        }
    } else {
        println!("HEAD: (not set)");
    }
    println!("---------------------");
}

// --- MAIN TEST FUNCTION ---

#[tokio::main]
async fn main() {
    println!("--- Git VFS Live Test: Swarm of Nodes with Varied Clone Times and Depths ---");
    println!("Press Ctrl-C or type 'q' and press Enter to stop.");

    // --- SETUP FOR GRACEFUL SHUTDOWN ---
    let running = Arc::new(AtomicBool::new(true));
    let r_signal = running.clone();
    let r_keypress = running.clone();

    // --- CENTRAL REPOSITORY INITIALIZATION ---
    let mut _central_repo = GitVfs::new();
    let _main_ref = "refs/heads/main";

    // Spawn a task to listen for Ctrl-C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        println!("\nCtrl-C received, shutting down.");
        r_signal.store(false, Ordering::SeqCst);
    });

    // Spawn a thread to listen for 'q' key press
    std::thread::spawn(move || {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            if line.unwrap_or_default().trim() == "q" {
                println!("\n'q' received, shutting down.");
                r_keypress.store(false, Ordering::SeqCst);
                break;
            }
        }
    });

    let mut commit_counter = 1;

    // --- MAIN LOOP for continuous commits and sync ---
    while running.load(Ordering::SeqCst) {
        println!("--- Starting loop iteration ---");
        println!("\n--- Cycle {} ---", commit_counter);
        println!("  -> Starting sleep...");



        sleep(Duration::from_secs(3)).await;
        println!("  -> Sleep finished. Continuing to next cycle.");
        commit_counter += 1;
        println!("--- Loop iteration finished ---");
    }

    println!("\n--- Live Test Finished ---");
}
