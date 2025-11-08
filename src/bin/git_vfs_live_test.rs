// --- IMPORTS ---
use git_vfs::GitVfs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

// --- HELPER FUNCTIONS ---

// Helper to print the state of a GitVfs instance
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
    println!("--- Git VFS Live Test: 2-Node Continuous Git History Exchange ---");
    println!("Press Ctrl-C or type 'q' and press Enter to stop.");

    // --- SETUP FOR GRACEFUL SHUTDOWN ---
    let running = Arc::new(AtomicBool::new(true));
    let r_signal = running.clone();
    let r_keypress = running.clone();

    // Spawn a task to listen for Ctrl-C
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl-c");
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

    // --- NODE INITIALIZATION ---
    let mut node1_vfs = GitVfs::new();
    let mut node2_vfs = GitVfs::new();
    let main_ref = "refs/heads/main";

    // --- INITIAL COMMIT ON NODE 1 and SYNC to NODE 2 ---
    println!("\n--- Initializing Node 1 with first commit ---");
    let initial_content = b"Initial commit from Node 1";
    let initial_hash = node1_vfs.create_blob(initial_content).unwrap();
    node1_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node1_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node1_vfs, "Node 1");

    println!("\n--- Cloning Node 1's state to Node 2 ---");
    let obj_data = node1_vfs.get_object(&initial_hash).unwrap();
    node2_vfs.create_object(&initial_hash, &obj_data).unwrap();
    node2_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node2_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node2_vfs, "Node 2");

    let mut commit_counter = 1;

    // --- MAIN LOOP for continuous commits and sync ---
    while running.load(Ordering::SeqCst) {
        println!("\n--- Cycle {} ---", commit_counter);

        // --- Node 1 creates a new commit ---
        let node1_content = format!("Node 1, commit #{}", commit_counter);
        let node1_hash = node1_vfs.create_blob(node1_content.as_bytes()).unwrap();
        node1_vfs.update_ref(main_ref, &node1_hash).unwrap();
        println!("Node 1 created new commit.");
        print_vfs_state(&node1_vfs, "Node 1");

        // --- Node 2 creates a new commit ---
        let node2_content = format!("Node 2, commit #{}", commit_counter);
        let node2_hash = node2_vfs.create_blob(node2_content.as_bytes()).unwrap();
        node2_vfs.update_ref(main_ref, &node2_hash).unwrap();
        println!("Node 2 created new commit.");
        print_vfs_state(&node2_vfs, "Node 2");

        // --- Simulate Syncing ---
        // Node 2 fetches from Node 1
        let n1_obj_data = node1_vfs.get_object(&node1_hash).unwrap();
        node2_vfs.create_object(&node1_hash, &n1_obj_data).unwrap();
        // In a real scenario, nodes would decide how to merge. Here we'll just have Node 2
        // arbitrarily decide to update its main ref to Node 1's version for simplicity.
        node2_vfs.update_ref(main_ref, &node1_hash).unwrap();
        println!("Node 2 synced from Node 1.");

        // Node 1 fetches from Node 2
        let n2_obj_data = node2_vfs.get_object(&node2_hash).unwrap();
        node1_vfs.create_object(&node2_hash, &n2_obj_data).unwrap();
        println!("Node 1 synced from Node 2 (object only).");

        println!("\n--- State after sync cycle {}", commit_counter);
        print_vfs_state(&node1_vfs, "Node 1 (Final)");
        print_vfs_state(&node2_vfs, "Node 2 (Final)");

        sleep(Duration::from_secs(3)).await;
        commit_counter += 1;
    }

    println!("\n--- Live Test Finished ---");
}
