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

// Helper function to print git log for a given branch
fn print_git_log(vfs: &GitVfs, node_name: &str, branch_name: &str) {
    println!("--- {} Git Log ({}) ---", node_name, branch_name);
    match vfs.get_ref(branch_name) {
        Ok(head_hash) => {
            match vfs.walk_history(&head_hash) {
                Ok(hashes) => {
                    if hashes.is_empty() {
                        println!("  (No commits)");
                    } else {
                        for hash in hashes {
                            match vfs.read_object(&hash) {
                                Ok(git_vfs::GitObject::Commit(commit)) => {
                                    // Format: commit <hash>\nAuthor: <author>\n\n    <message>\n
                                    println!("commit {}", hash);
                                    println!("Author: {}", commit.author);
                                    // Indent message lines
                                    for line in commit.message.lines() {
                                        println!("    {}", line);
                                    }
                                    println!(); // Blank line between commits
                                }
                                Ok(_) => println!("  (Non-commit object at hash: {})", hash),
                                Err(e) => println!("  Error reading object {}: {:?}", hash, e),
                            }
                        }
                    }
                }
                Err(e) => println!("  Error walking history for {}: {:?}", branch_name, e),
            }
        }
        Err(git_vfs::GitVfsError::NotFound) => println!("  Branch '{}' not found.", branch_name),
        Err(e) => println!("  Error getting ref for {}: {:?}", branch_name, e),
    }
    println!("--------------------------");
}


// --- MAIN TEST FUNCTION ---

#[tokio::main] async fn main() {
    println!("--- Git VFS Live Test: 2-Node Continuous Git History Exchange ---");
    println!("Press Ctrl-C or type 'q' and press Enter to stop.");

    // --- SETUP FOR GRACEFUL SHUTDOWN ---
    let running = Arc::new(AtomicBool::new(true));
    let r_signal = running.clone();
    let r_keypress = running.clone();

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

    let mut node1_vfs = GitVfs::new();
    let mut node2_vfs = GitVfs::new();
    // Add two new nodes for the extended test
    let mut node3_vfs = GitVfs::new();
    let mut node4_vfs = GitVfs::new();
    let main_ref = "refs/heads/main";

    // --- INITIAL COMMIT ON NODE 1 and SYNC to NODE 2 ---
    println!("\n--- Initializing Node 1 with first commit ---");
    let initial_content = b"Initial commit from Node 1";
    let initial_hash = node1_vfs.create_blob(initial_content).unwrap();
    node1_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node1_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node1_vfs, "Node 1");
    print_git_log(&node1_vfs, "Node 1", main_ref); // Log after initial commit

    println!("\n--- Cloning Node 1's state to Node 2 ---");
    let obj_data = node1_vfs.get_object(&initial_hash).unwrap();
    node2_vfs.create_object(&initial_hash, &obj_data).unwrap();
    node2_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node2_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node2_vfs, "Node 2");
    print_git_log(&node2_vfs, "Node 2", main_ref); // Log after cloning

    let mut commit_counter = 1;
    let mut new_nodes_initialized = false; // Flag to track initialization of new nodes

    // --- MAIN LOOP for continuous commits and sync ---
    while running.load(Ordering::SeqCst) {
        println!("\n--- Cycle {} ---", commit_counter);

        // --- Initialize new nodes after 10 cycles ---
        if commit_counter == 10 && !new_nodes_initialized {
            println!("\n--- Initializing Node 3 and Node 4 after 10 cycles ---");

            // Clone Node 1's current state into Node 3
            let n1_current_obj_hash = node1_vfs.get_ref(main_ref).unwrap(); // Get current head hash
            let n1_current_obj_data = node1_vfs.get_object(&n1_current_obj_hash).unwrap();
            node3_vfs.create_object(&n1_current_obj_hash, &n1_current_obj_data).unwrap();
            node3_vfs.create_ref(main_ref, &n1_current_obj_hash).unwrap();
            node3_vfs.set_head(main_ref).unwrap();
            print_vfs_state(&node3_vfs, "Node 3");
            print_git_log(&node3_vfs, "Node 3", main_ref); // Log after initialization

            // Clone Node 1's current state into Node 4
            let n1_current_obj_hash_for_n4 = node1_vfs.get_ref(main_ref).unwrap(); // Re-fetch in case of concurrent changes (though not expected here)
            let n1_current_obj_data_for_n4 = node1_vfs.get_object(&n1_current_obj_hash_for_n4).unwrap();
            node4_vfs.create_object(&n1_current_obj_hash_for_n4, &n1_current_obj_data_for_n4).unwrap();
            node4_vfs.create_ref(main_ref, &n1_current_obj_hash_for_n4).unwrap();
            node4_vfs.set_head(main_ref).unwrap();
            print_vfs_state(&node4_vfs, "Node 4");
            print_git_log(&node4_vfs, "Node 4", main_ref); // Log after initialization

            new_nodes_initialized = true;
            println!("--- New nodes (Node 3 and Node 4) initialized and cloned from Node 1 ---");
        }

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

        // --- Node 3 creates a new commit (if initialized) ---
        if new_nodes_initialized {
            let node3_content = format!("Node 3, commit #{}", commit_counter);
            let node3_hash = node3_vfs.create_blob(node3_content.as_bytes()).unwrap();
            node3_vfs.update_ref(main_ref, &node3_hash).unwrap();
            println!("Node 3 created new commit.");
            print_vfs_state(&node3_vfs, "Node 3");
        }

        // --- Node 4 creates a new commit (if initialized) ---
        if new_nodes_initialized {
            let node4_content = format!("Node 4, commit #{}", commit_counter);
            let node4_hash = node4_vfs.create_blob(node4_content.as_bytes()).unwrap();
            node4_vfs.update_ref(main_ref, &node4_hash).unwrap();
            println!("Node 4 created new commit.");
            print_vfs_state(&node4_vfs, "Node 4");
        }


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

        // Node 3 syncs from Node 1 (if initialized)
        if new_nodes_initialized {
            let n1_obj_data_for_n3 = node1_vfs.get_object(&node1_hash).unwrap();
            node3_vfs.create_object(&node1_hash, &n1_obj_data_for_n3).unwrap();
            node3_vfs.update_ref(main_ref, &node1_hash).unwrap(); // Node 3 updates to Node 1's latest
            println!("Node 3 synced from Node 1.");
        }

        // Node 4 syncs from Node 2 (if initialized)
        if new_nodes_initialized {
            let n2_obj_data_for_n4 = node2_vfs.get_object(&node2_hash).unwrap();
            node4_vfs.create_object(&node2_hash, &n2_obj_data_for_n4).unwrap();
            node4_vfs.update_ref(main_ref, &node2_hash).unwrap(); // Node 4 updates to Node 2's latest
            println!("Node 4 synced from Node 2.");
        }


        println!("\n--- State after sync cycle {}", commit_counter);
        print_vfs_state(&node1_vfs, "Node 1 (Final)");
        print_vfs_state(&node2_vfs, "Node 2 (Final)");
        if new_nodes_initialized {
            print_vfs_state(&node3_vfs, "Node 3 (Final)");
            print_vfs_state(&node4_vfs, "Node 4 (Final)");
        }

        // --- Print Git Log for each node ---
        println!("\n--- Git Log after sync cycle {} ---", commit_counter);
        print_git_log(&node1_vfs, "Node 1", main_ref);
        print_git_log(&node2_vfs, "Node 2", main_ref);
        if new_nodes_initialized {
            print_git_log(&node3_vfs, "Node 3", main_ref);
            print_git_log(&node4_vfs, "Node 4", main_ref);
        }


        sleep(Duration::from_secs(3)).await;
        commit_counter += 1;
    }

    println!("\n--- Live Test Finished ---");
}