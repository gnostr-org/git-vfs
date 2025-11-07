// --- IMPORTS ---
use git_vfs::{GitVfs, Diff};
use git_vfs::memory_vfs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use clearscreen::clear;

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

// Helper to print the state of a Diff instance
fn print_diff_state(diff: &Diff, source_node: &str, target_node: &str) {
    println!("--- Diff from {} to {} ---", source_node, target_node);
    let mut has_diff = false;

    if let Some((old_head, new_head)) = &diff.head_changed {
        has_diff = true;
        match old_head {
            Some(h) => println!("- HEAD: {}", h),
            None => {},
        }
        match new_head {
            Some(h) => println!("+ HEAD: {}", h),
            None => {},
        }
    }

    if !diff.refs_added.is_empty() {
        has_diff = true;
        for (ref_name, hash) in &diff.refs_added {
            println!("+ {}: {}", ref_name, hash);
        }
    }
    if !diff.refs_removed.is_empty() {
        has_diff = true;
        for (ref_name, hash) in &diff.refs_removed {
            println!("- {}: {}", ref_name, hash);
        }
    }
    if !diff.refs_updated.is_empty() {
        has_diff = true;
        for (ref_name, (old_hash, new_hash)) in &diff.refs_updated {
            println!("- {}: {}", ref_name, old_hash);
            println!("+ {}: {}", ref_name, new_hash);
        }
    }
    if !has_diff {
        println!("No differences.");
    }
    println!("--------------------------");
}

// --- MAIN TEST FUNCTION ---

#[tokio::main]
async fn main() {
    println!("--- Git VFS Live Test: 4-Node Continuous Git History Exchange ---");
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
    let mut node3_vfs = GitVfs::new();
    let mut node4_vfs = GitVfs::new();
    let main_ref = "refs/heads/main";

    // --- Test Memory VFS functionality ---
    println!("\n--- Testing Memory VFS ---");
    match memory_vfs::create_and_test_memory_fs() {
        Ok(_) => println!("Memory VFS test passed."),
        Err(e) => eprintln!("Memory VFS test failed: {:?}", e),
    }
    println!("--------------------------");

    // --- INITIAL COMMIT ON NODE 1 and SYNC to NODE 2 ---
    println!("\n--- Initializing Node 1 with first commit ---");
    let initial_content = b"feat(init): Initial commit from Node 1";
    let initial_hash = node1_vfs.create_blob(initial_content).unwrap();
    node1_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node1_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node1_vfs, "Node 1");

    println!("\n--- Cloning Node 1's state to Node 2, Node 3, and Node 4 ---");
    let obj_data = node1_vfs.get_object(&initial_hash).unwrap();
    node2_vfs.create_object(&initial_hash, &obj_data).unwrap();
    node2_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node2_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node2_vfs, "Node 2");

    node3_vfs.create_object(&initial_hash, &obj_data).unwrap();
    node3_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node3_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node3_vfs, "Node 3");

    node4_vfs.create_object(&initial_hash, &obj_data).unwrap();
    node4_vfs.create_ref(main_ref, &initial_hash).unwrap();
    node4_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node4_vfs, "Node 4");

    let mut commit_counter = 1;

    // --- MAIN LOOP for continuous commits and sync ---
    while running.load(Ordering::SeqCst) {
        clear().unwrap();
        println!("\n--- Cycle {} ---", commit_counter);

        // --- Node 1 creates a new commit ---
        let node1_content = format!("feat(node1): add commit #{}", commit_counter);
        let node1_hash = node1_vfs.create_blob(node1_content.as_bytes()).unwrap();
        node1_vfs.update_ref(main_ref, &node1_hash).unwrap();
        println!("Node 1 created new commit.");
        print_vfs_state(&node1_vfs, "Node 1");

        println!("\n--- Intermediate Diffs after Node 1 commit ---");
        let diff1_2_pre_n2 = node1_vfs.diff(&node2_vfs);
        print_diff_state(&diff1_2_pre_n2, "Node 1", "Node 2 (pre-commit)");
        let diff1_3_pre_n3 = node1_vfs.diff(&node3_vfs);
        print_diff_state(&diff1_3_pre_n3, "Node 1", "Node 3 (pre-commit)");
        let diff1_4_pre_n4 = node1_vfs.diff(&node4_vfs);
        print_diff_state(&diff1_4_pre_n4, "Node 1", "Node 4 (pre-commit)");

        // --- Node 2 creates a new commit ---
        let node2_content = format!("feat(node2): add commit #{}", commit_counter);
        let node2_hash = node2_vfs.create_blob(node2_content.as_bytes()).unwrap();
        node2_vfs.update_ref(main_ref, &node2_hash).unwrap();
        println!("Node 2 created new commit.");
        print_vfs_state(&node2_vfs, "Node 2");

        // Node 2 creates a feature branch every 2 cycles
        if commit_counter % 2 == 0 {
            let feature_ref = format!("refs/heads/feature-{}", commit_counter);
            node2_vfs.create_ref(&feature_ref, &node2_hash).unwrap();
            println!("Node 2 created feature branch: {}", feature_ref);
        }

        println!("\n--- Intermediate Diffs after Node 2 commit ---");
        let diff1_2_post_n2 = node1_vfs.diff(&node2_vfs);
        print_diff_state(&diff1_2_post_n2, "Node 1", "Node 2 (post-commit)");

        // --- Node 3 creates a new commit ---
        let node3_content = format!("feat(node3): add commit #{}", commit_counter);
        let node3_hash = node3_vfs.create_blob(node3_content.as_bytes()).unwrap();
        node3_vfs.update_ref(main_ref, &node3_hash).unwrap();
        println!("Node 3 created new commit.");
        print_vfs_state(&node3_vfs, "Node 3");

        // Node 3 creates a temporary branch and then deletes it after 3 cycles
        if commit_counter % 3 == 1 {
            let temp_ref = format!("refs/heads/temp-{}", commit_counter);
            node3_vfs.create_ref(&temp_ref, &node3_hash).unwrap();
            println!("Node 3 created temporary branch: {}", temp_ref);
        } else if commit_counter % 3 == 0 && commit_counter > 0 {
            let temp_ref_to_remove = format!("refs/heads/temp-{}", commit_counter - 2);
            if node3_vfs.get_ref(&temp_ref_to_remove).is_ok() {
                node3_vfs.remove_ref(&temp_ref_to_remove).unwrap();
                println!("Node 3 removed temporary branch: {}", temp_ref_to_remove);
            }
        }

        println!("\n--- Intermediate Diffs after Node 3 commit ---");
        let diff1_3_post_n3 = node1_vfs.diff(&node3_vfs);
        print_diff_state(&diff1_3_post_n3, "Node 1", "Node 3 (post-commit)");

        // --- Node 4 creates a new commit ---
        let node4_content = format!("feat(node4): add commit #{}", commit_counter);
        let node4_hash = node4_vfs.create_blob(node4_content.as_bytes()).unwrap();
        node4_vfs.update_ref(main_ref, &node4_hash).unwrap();
        println!("Node 4 created new commit.");
        print_vfs_state(&node4_vfs, "Node 4");

        // Node 4 creates a new branch and switches HEAD every 2 cycles
        if commit_counter % 2 == 1 {
            let new_branch_ref = format!("refs/heads/experimental-{}", commit_counter);
            node4_vfs.create_ref(&new_branch_ref, &node4_hash).unwrap();
            node4_vfs.set_head(&new_branch_ref).unwrap();
            println!("Node 4 created and switched to branch: {}", new_branch_ref);
        } else {
            // Switch back to main on even cycles to create more HEAD changes
            node4_vfs.set_head(main_ref).unwrap();
            println!("Node 4 switched HEAD back to: {}", main_ref);
        }

        println!("\n--- Intermediate Diffs after Node 4 commit ---");
        let diff1_4_post_n4 = node1_vfs.diff(&node4_vfs);
        print_diff_state(&diff1_4_post_n4, "Node 1", "Node 4 (post-commit)");

        // --- Simulate Syncing ---
        // Node 2 fetches from Node 1
        let n1_obj_data = node1_vfs.get_object(&node1_hash).unwrap();
        node2_vfs.create_object(&node1_hash, &n1_obj_data).unwrap();
        node2_vfs.update_ref(main_ref, &node1_hash).unwrap();
        println!("Node 2 synced from Node 1.");

        // Node 3 fetches from Node 1
        node3_vfs.create_object(&node1_hash, &n1_obj_data).unwrap();
        node3_vfs.update_ref(main_ref, &node1_hash).unwrap();
        println!("Node 3 synced from Node 1.");

        // Node 4 fetches from Node 1
        node4_vfs.create_object(&node1_hash, &n1_obj_data).unwrap();
        node4_vfs.update_ref(main_ref, &node1_hash).unwrap();
        println!("Node 4 synced from Node 1.");

        // Node 1 fetches from Node 2, Node 3, and Node 4 (object only for simplicity)
        let n2_obj_data = node2_vfs.get_object(&node2_hash).unwrap();
        node1_vfs.create_object(&node2_hash, &n2_obj_data).unwrap();
        let n3_obj_data = node3_vfs.get_object(&node3_hash).unwrap();
        node1_vfs.create_object(&node3_hash, &n3_obj_data).unwrap();
        let n4_obj_data = node4_vfs.get_object(&node4_hash).unwrap();
        node1_vfs.create_object(&node4_hash, &n4_obj_data).unwrap();
        println!("Node 1 synced from Node 2, Node 3, and Node 4 (objects only).");

        println!("\n--- State after sync cycle {}", commit_counter);
        print_vfs_state(&node1_vfs, "Node 1 (Final)");
        print_vfs_state(&node2_vfs, "Node 2 (Final)");
        print_vfs_state(&node3_vfs, "Node 3 (Final)");
        print_vfs_state(&node4_vfs, "Node 4 (Final)");

        // --- Print Diffs ---
        println!("\n--- Diffs after sync cycle {} ---", commit_counter);
        let diff1_2 = node1_vfs.diff(&node2_vfs);
        print_diff_state(&diff1_2, "Node 1", "Node 2");

        let diff2_1 = node2_vfs.diff(&node1_vfs);
        print_diff_state(&diff2_1, "Node 2", "Node 1");

        let diff1_3 = node1_vfs.diff(&node3_vfs);
        print_diff_state(&diff1_3, "Node 1", "Node 3");

        let diff3_1 = node3_vfs.diff(&node1_vfs);
        print_diff_state(&diff3_1, "Node 3", "Node 1");

        let diff1_4 = node1_vfs.diff(&node4_vfs);
        print_diff_state(&diff1_4, "Node 1", "Node 4");

        let diff4_1 = node4_vfs.diff(&node1_vfs);
        print_diff_state(&diff4_1, "Node 4", "Node 1");

        sleep(Duration::from_secs(3)).await;
        commit_counter += 1;
    }

    println!("\n--- Live Test Finished ---");
}
