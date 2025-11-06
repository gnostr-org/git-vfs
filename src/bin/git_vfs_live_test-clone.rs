
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
    let initial_blob_hash = node1_vfs.create_blob(initial_content).unwrap();

    let mut initial_tree_entries = std::collections::HashMap::new();
    initial_tree_entries.insert("file1.txt".to_string(), (initial_blob_hash.clone(), git_vfs::GitObjectKind::Blob));
    let initial_tree_hash = node1_vfs.create_tree(initial_tree_entries).unwrap();

    let initial_commit_hash = match node1_vfs.create_commit(
        "Node 1 Author",
        "Initial commit from Node 1",
        &initial_tree_hash,
        vec![],
    ) {
        Ok(hash) => hash,
        Err(git_vfs::GitVfsError::AlreadyExists) => {
            // If it already exists, we need to retrieve its hash. This is a simplification for the test.
            // In a real scenario, you'd likely have a way to query for existing commits.
            // For now, we'll re-calculate the hash based on the content, assuming it's consistent.
            let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                author: "Node 1 Author".to_string(),
                message: "Initial commit from Node 1".to_string(),
                tree_hash: initial_tree_hash.clone(),
                parent_hashes: vec![],
            });
            node1_vfs.data_sha256(&commit_object.to_vec())
        },
        Err(e) => panic!("Failed to create commit: {:?}", e),
    };
    node1_vfs.create_ref(main_ref, &initial_commit_hash).unwrap();
    node1_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node1_vfs, "Node 1");

    println!("\n--- Cloning Node 1's state to Node 2 ---");
    let node1_head_commit_hash = node1_vfs.get_ref(main_ref).unwrap();
    let node1_head_commit = match node1_vfs.get_object(&node1_head_commit_hash).unwrap() {
        git_vfs::GitObject::Commit(c) => c,
        _ => panic!("Expected commit object"),
    };

    // Fetch and create tree object
    let node1_tree = match node1_vfs.get_object(&node1_head_commit.tree_hash).unwrap() {
        git_vfs::GitObject::Tree(t) => t,
        _ => panic!("Expected tree object"),
    };
    match node2_vfs.create_tree(node1_tree.clone()) {
        Ok(_) => {},
        Err(git_vfs::GitVfsError::AlreadyExists) => {},
        Err(e) => panic!("Failed to create tree: {:?}", e),
    };

    // Fetch and create blob objects (assuming only one for simplicity)
    for (_, (blob_hash, _)) in node1_tree.iter() {
        let blob_data = match node1_vfs.get_object(blob_hash).unwrap() {
            git_vfs::GitObject::Blob(b) => b,
            _ => panic!("Expected blob object"),
        };
        match node2_vfs.create_blob(&blob_data) {
            Ok(_) => {},
            Err(git_vfs::GitVfsError::AlreadyExists) => {},
            Err(e) => panic!("Failed to create blob: {:?}", e),
        };
    }

    // Create the commit object in Node 2
    let node2_new_commit_hash = match node2_vfs.create_commit(
        &node1_head_commit.author,
        &node1_head_commit.message,
        &node1_head_commit.tree_hash,
        node1_head_commit.parent_hashes.clone(),
    ) {
        Ok(hash) => hash,
        Err(git_vfs::GitVfsError::AlreadyExists) => {
            // If it already exists, we need to retrieve its hash.
            // Re-calculate the hash based on the content, assuming it's consistent.
            let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                author: node1_head_commit.author.clone(),
                message: node1_head_commit.message.clone(),
                tree_hash: node1_head_commit.tree_hash.clone(),
                parent_hashes: node1_head_commit.parent_hashes.clone(),
            });
            node2_vfs.data_sha256(&commit_object.to_vec())
        },
        Err(e) => panic!("Failed to create commit: {:?}", e),
    };

    node2_vfs.create_ref(main_ref, &node1_head_commit_hash).unwrap();
    node2_vfs.set_head(main_ref).unwrap();
    print_vfs_state(&node2_vfs, "Node 2");

    let mut commit_counter = 1;
    let mut new_nodes_initialized = false; // Flag to track initialization of new nodes

    // --- MAIN LOOP for continuous commits and sync ---
    while running.load(Ordering::SeqCst) {
        println!("\n--- Cycle {} ---", commit_counter);

        // --- Initialize new nodes after 10 cycles ---
        if commit_counter == 10 && !new_nodes_initialized {
            println!("\n--- Initializing Node 3 and Node 4 after 10 cycles ---");

            // Clone Node 1's current state into Node 3
            let n1_current_commit_hash = node1_vfs.get_ref(main_ref).unwrap(); // Get current head hash
            let n1_current_commit = match node1_vfs.get_object(&n1_current_commit_hash).unwrap() {
                git_vfs::GitObject::Commit(c) => c,
                _ => panic!("Expected commit object"),
            };

            // Fetch and create tree object
            let n1_current_tree = match node1_vfs.get_object(&n1_current_commit.tree_hash).unwrap() {
                git_vfs::GitObject::Tree(t) => t,
                _ => panic!("Expected tree object"),
            };
            match node3_vfs.create_tree(n1_current_tree.clone()) {
                Ok(_) => {},
                Err(git_vfs::GitVfsError::AlreadyExists) => {},
                Err(e) => panic!("Failed to create tree: {:?}", e),
            };

            // Fetch and create blob objects
            for (_, (blob_hash, _)) in n1_current_tree.iter() {
                let blob_data = match node1_vfs.get_object(blob_hash).unwrap() {
                    git_vfs::GitObject::Blob(b) => b,
                    _ => panic!("Expected blob object"),
                };
                node3_vfs.create_blob(&blob_data).unwrap();
            }

            // Create the commit object in Node 3
            let node3_new_commit_hash = match node3_vfs.create_commit(
                &n1_current_commit.author,
                &n1_current_commit.message,
                &n1_current_commit.tree_hash,
                n1_current_commit.parent_hashes.clone(),
            ) {
                Ok(hash) => hash,
                Err(git_vfs::GitVfsError::AlreadyExists) => {
                    // If it already exists, we need to retrieve its hash.
                    // Re-calculate the hash based on the content, assuming it's consistent.
                    let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                        author: n1_current_commit.author.clone(),
                        message: n1_current_commit.message.clone(),
                        tree_hash: n1_current_commit.tree_hash.clone(),
                        parent_hashes: n1_current_commit.parent_hashes.clone(),
                    });
                    node3_vfs.data_sha256(&commit_object.to_vec())
                },
                Err(e) => panic!("Failed to create commit: {:?}", e),
            };

            node3_vfs.create_ref(main_ref, &n1_current_commit_hash).unwrap();
            node3_vfs.set_head(main_ref).unwrap();
            print_vfs_state(&node3_vfs, "Node 3");

            // Clone Node 1's current state into Node 4
            let n1_current_commit_hash_for_n4 = node1_vfs.get_ref(main_ref).unwrap(); // Re-fetch in case of concurrent changes (though not expected here)
            let n1_current_commit_for_n4 = match node1_vfs.get_object(&n1_current_commit_hash_for_n4).unwrap() {
                git_vfs::GitObject::Commit(c) => c,
                _ => panic!("Expected commit object"),
            };

            // Fetch and create tree object
            let n1_current_tree_for_n4 = match node1_vfs.get_object(&n1_current_commit_for_n4.tree_hash).unwrap() {
                git_vfs::GitObject::Tree(t) => t,
                _ => panic!("Expected tree object"),
            };
            node4_vfs.create_tree(n1_current_tree_for_n4.clone()).unwrap();

            // Fetch and create blob objects
            for (_, (blob_hash, _)) in n1_current_tree_for_n4.iter() {
                let blob_data = match node1_vfs.get_object(blob_hash).unwrap() {
                    git_vfs::GitObject::Blob(b) => b,
                    _ => panic!("Expected blob object"),
                };
                node4_vfs.create_blob(&blob_data).unwrap();
            }

            // Create the commit object in Node 4
            let node4_new_commit_hash = match node4_vfs.create_commit(
                &n1_current_commit_for_n4.author,
                &n1_current_commit_for_n4.message,
                &n1_current_commit_for_n4.tree_hash,
                n1_current_commit_for_n4.parent_hashes.clone(),
            ) {
                Ok(hash) => hash,
                Err(git_vfs::GitVfsError::AlreadyExists) => {
                    // If it already exists, we need to retrieve its hash.
                    // Re-calculate the hash based on the content, assuming it's consistent.
                    let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                        author: n1_current_commit_for_n4.author.clone(),
                        message: n1_current_commit_for_n4.message.clone(),
                        tree_hash: n1_current_commit_for_n4.tree_hash.clone(),
                        parent_hashes: n1_current_commit_for_n4.parent_hashes.clone(),
                    });
                    node4_vfs.data_sha256(&commit_object.to_vec())
                },
                Err(e) => panic!("Failed to create commit: {:?}", e),
            };

            node4_vfs.create_ref(main_ref, &n1_current_commit_hash_for_n4).unwrap();
            node4_vfs.set_head(main_ref).unwrap();
            print_vfs_state(&node4_vfs, "Node 4");

            new_nodes_initialized = true;
            println!("--- New nodes (Node 3 and Node 4) initialized and cloned from Node 1 ---");
        }

        // --- Node 1 creates a new commit ---
        let node1_prev_commit_hash = node1_vfs.get_ref(main_ref).unwrap();
        let node1_prev_commit = match node1_vfs.get_object(&node1_prev_commit_hash).unwrap() {
            git_vfs::GitObject::Commit(c) => c,
            _ => panic!("Expected commit object"),
        };
        let node1_prev_tree = node1_vfs.get_tree(&node1_prev_commit.tree_hash).unwrap();

        let node1_content = format!("Node 1, commit #{}", commit_counter);
        let node1_blob_hash = node1_vfs.create_blob(node1_content.as_bytes()).unwrap();

        let mut node1_new_tree_entries = node1_prev_tree.clone();
        node1_new_tree_entries.insert(format!("file{}.txt", commit_counter), (node1_blob_hash.clone(), git_vfs::GitObjectKind::Blob));
        let node1_new_tree_hash = node1_vfs.create_tree(node1_new_tree_entries).unwrap();

        let node1_new_commit_hash = match node1_vfs.create_commit(
            "Node 1 Author",
            &node1_content,
            &node1_new_tree_hash,
            vec![node1_prev_commit_hash.clone()],
        ) {
            Ok(hash) => hash,
            Err(git_vfs::GitVfsError::AlreadyExists) => {
                // If it already exists, we need to retrieve its hash.
                // Re-calculate the hash based on the content, assuming it's consistent.
                let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                    author: "Node 1 Author".to_string(),
                    message: node1_content.clone(),
                    tree_hash: node1_new_tree_hash.clone(),
                    parent_hashes: vec![node1_prev_commit_hash.clone()],
                });
                node1_vfs.data_sha256(&commit_object.to_vec())
            },
            Err(e) => panic!("Failed to create commit: {:?}", e),
        };
        node1_vfs.update_ref(main_ref, &node1_new_commit_hash).unwrap();
        println!("Node 1 created new commit.");
        print_vfs_state(&node1_vfs, "Node 1");

        // --- Node 2 creates a new commit ---
        let node2_prev_commit_hash = node2_vfs.get_ref(main_ref).unwrap();
        let node2_prev_commit = match node2_vfs.get_object(&node2_prev_commit_hash).unwrap() {
            git_vfs::GitObject::Commit(c) => c,
            _ => panic!("Expected commit object"),
        };
        let node2_prev_tree = node2_vfs.get_tree(&node2_prev_commit.tree_hash).unwrap();

        let node2_content = format!("Node 2, commit #{}", commit_counter);
        let node2_blob_hash = node2_vfs.create_blob(node2_content.as_bytes()).unwrap();

        let mut node2_new_tree_entries = node2_prev_tree.clone();
        node2_new_tree_entries.insert(format!("file{}.txt", commit_counter), (node2_blob_hash.clone(), git_vfs::GitObjectKind::Blob));
        let node2_new_tree_hash = node2_vfs.create_tree(node2_new_tree_entries).unwrap();

        let node2_new_commit_hash = match node2_vfs.create_commit(
            "Node 2 Author",
            &node2_content,
            &node2_new_tree_hash,
            vec![node2_prev_commit_hash.clone()],
        ) {
            Ok(hash) => hash,
            Err(git_vfs::GitVfsError::AlreadyExists) => {
                // If it already exists, we need to retrieve its hash.
                // Re-calculate the hash based on the content, assuming it's consistent.
                let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                    author: "Node 2 Author".to_string(),
                    message: node2_content.clone(),
                    tree_hash: node2_new_tree_hash.clone(),
                    parent_hashes: vec![node2_prev_commit_hash.clone()],
                });
                node2_vfs.data_sha256(&commit_object.to_vec())
            },
            Err(e) => panic!("Failed to create commit: {:?}", e),
        };
        node2_vfs.update_ref(main_ref, &node2_new_commit_hash).unwrap();
        println!("Node 2 created new commit.");
        print_vfs_state(&node2_vfs, "Node 2");

        // --- Node 3 creates a new commit (if initialized) ---
        if new_nodes_initialized {
            let node3_prev_commit_hash = node3_vfs.get_ref(main_ref).unwrap();
            let node3_prev_commit = match node3_vfs.get_object(&node3_prev_commit_hash).unwrap() {
                git_vfs::GitObject::Commit(c) => c,
                _ => panic!("Expected commit object"),
            };
            let node3_prev_tree = node3_vfs.get_tree(&node3_prev_commit.tree_hash).unwrap();

            let node3_content = format!("Node 3, commit #{}", commit_counter);
            let node3_blob_hash = node3_vfs.create_blob(node3_content.as_bytes()).unwrap();

            let mut node3_new_tree_entries = node3_prev_tree.clone();
            node3_new_tree_entries.insert(format!("file{}.txt", commit_counter), (node3_blob_hash.clone(), git_vfs::GitObjectKind::Blob));
            let node3_new_tree_hash = node3_vfs.create_tree(node3_new_tree_entries).unwrap();

            let node3_new_commit_hash = match node3_vfs.create_commit(
                "Node 3 Author",
                &node3_content,
                &node3_new_tree_hash,
                vec![node3_prev_commit_hash.clone()],
            ) {
                Ok(hash) => hash,
                Err(git_vfs::GitVfsError::AlreadyExists) => {
                    // If it already exists, we need to retrieve its hash.
                    // Re-calculate the hash based on the content, assuming it's consistent.
                    let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                        author: "Node 3 Author".to_string(),
                        message: node3_content.clone(),
                        tree_hash: node3_new_tree_hash.clone(),
                        parent_hashes: vec![node3_prev_commit_hash.clone()],
                    });
                    node3_vfs.data_sha256(&commit_object.to_vec())
                },
                Err(e) => panic!("Failed to create commit: {:?}", e),
            };
            node3_vfs.update_ref(main_ref, &node3_new_commit_hash).unwrap();
            println!("Node 3 created new commit.");
            print_vfs_state(&node3_vfs, "Node 3");
        }

        // --- Node 4 creates a new commit (if initialized) ---
        if new_nodes_initialized {
            let node4_prev_commit_hash = node4_vfs.get_ref(main_ref).unwrap();
            let node4_prev_commit = match node4_vfs.get_object(&node4_prev_commit_hash).unwrap() {
                git_vfs::GitObject::Commit(c) => c,
                _ => panic!("Expected commit object"),
            };
            let node4_prev_tree = node4_vfs.get_tree(&node4_prev_commit.tree_hash).unwrap();

            let node4_content = format!("Node 4, commit #{}", commit_counter);
            let node4_blob_hash = node4_vfs.create_blob(node4_content.as_bytes()).unwrap();

            let mut node4_new_tree_entries = node4_prev_tree.clone();
            node4_new_tree_entries.insert(format!("file{}.txt", commit_counter), (node4_blob_hash.clone(), git_vfs::GitObjectKind::Blob));
            let node4_new_tree_hash = node4_vfs.create_tree(node4_new_tree_entries).unwrap();

            let node4_new_commit_hash = match node4_vfs.create_commit(
                "Node 4 Author",
                &node4_content,
                &node4_new_tree_hash,
                vec![node4_prev_commit_hash.clone()],
            ) {
                Ok(hash) => hash,
                Err(git_vfs::GitVfsError::AlreadyExists) => {
                    // If it already exists, we need to retrieve its hash.
                    // Re-calculate the hash based on the content, assuming it's consistent.
                    let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                        author: "Node 4 Author".to_string(),
                        message: node4_content.clone(),
                        tree_hash: node4_new_tree_hash.clone(),
                        parent_hashes: vec![node4_prev_commit_hash.clone()],
                    });
                    node4_vfs.data_sha256(&commit_object.to_vec())
                },
                Err(e) => panic!("Failed to create commit: {:?}", e),
            };
            node4_vfs.update_ref(main_ref, &node4_new_commit_hash).unwrap();
            println!("Node 4 created new commit.");
            print_vfs_state(&node4_vfs, "Node 4");
        }


        // --- Simulate Syncing ---
        // Node 2 fetches from Node 1
        let node1_latest_commit_hash = node1_vfs.get_ref(main_ref).unwrap();
        let node1_latest_commit = match node1_vfs.get_object(&node1_latest_commit_hash).unwrap() {
            git_vfs::GitObject::Commit(c) => c,
            _ => panic!("Expected commit object"),
        };

        // Fetch and create tree object
        let node1_tree = match node1_vfs.get_object(&node1_latest_commit.tree_hash).unwrap() {
            git_vfs::GitObject::Tree(t) => t,
            _ => panic!("Expected tree object"),
        };
        match node2_vfs.create_tree(node1_tree.clone()) {
        Ok(_) => {},
        Err(git_vfs::GitVfsError::AlreadyExists) => {},
        Err(e) => panic!("Failed to create tree: {:?}", e),
    };

        // Fetch and create blob objects
        for (_, (blob_hash, _)) in node1_tree.iter() {
            let blob_data = match node1_vfs.get_object(blob_hash).unwrap() {
                git_vfs::GitObject::Blob(b) => b,
                _ => panic!("Expected blob object"),
            };
            node2_vfs.create_blob(&blob_data).unwrap();
        }

        // Create the commit object in Node 2
        let node2_sync_commit_hash = match node2_vfs.create_commit(
            &node1_latest_commit.author,
            &node1_latest_commit.message,
            &node1_latest_commit.tree_hash,
            node1_latest_commit.parent_hashes.clone(),
        ) {
            Ok(hash) => hash,
            Err(git_vfs::GitVfsError::AlreadyExists) => {
                // If it already exists, we need to retrieve its hash.
                // Re-calculate the hash based on the content, assuming it's consistent.
                let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                    author: node1_latest_commit.author.clone(),
                    message: node1_latest_commit.message.clone(),
                    tree_hash: node1_latest_commit.tree_hash.clone(),
                    parent_hashes: node1_latest_commit.parent_hashes.clone(),
                });
                node2_vfs.data_sha256(&commit_object.to_vec())
            },
            Err(e) => panic!("Failed to create commit: {:?}", e),
        };

        node2_vfs.update_ref(main_ref, &node1_latest_commit_hash).unwrap();
        println!("Node 2 synced from Node 1.");

        // Node 1 fetches from Node 2
        let node2_latest_commit_hash = node2_vfs.get_ref(main_ref).unwrap();
        let node2_latest_commit = match node2_vfs.get_object(&node2_latest_commit_hash).unwrap() {
            git_vfs::GitObject::Commit(c) => c,
            _ => panic!("Expected commit object"),
        };

        // Fetch and create tree object
        let node2_tree = match node2_vfs.get_object(&node2_latest_commit.tree_hash).unwrap() {
            git_vfs::GitObject::Tree(t) => t,
            _ => panic!("Expected tree object"),
        };
        match node1_vfs.create_tree(node2_tree.clone()) {
            Ok(_) => {},
            Err(git_vfs::GitVfsError::AlreadyExists) => {},
            Err(e) => panic!("Failed to create tree: {:?}", e),
        };

        // Fetch and create blob objects
        for (_, (blob_hash, _)) in node2_tree.iter() {
            let blob_data = match node2_vfs.get_object(blob_hash).unwrap() {
                git_vfs::GitObject::Blob(b) => b,
                _ => panic!("Expected blob object"),
            };
            match node1_vfs.create_blob(&blob_data) {
                Ok(_) => {},
                Err(git_vfs::GitVfsError::AlreadyExists) => {},
                Err(e) => panic!("Failed to create blob: {:?}", e),
            };
        }

        // Create the commit object in Node 1
        let node1_sync_commit_hash = match node1_vfs.create_commit(
            &node2_latest_commit.author,
            &node2_latest_commit.message,
            &node2_latest_commit.tree_hash,
            node2_latest_commit.parent_hashes.clone(),
        ) {
            Ok(hash) => hash, // Assign hash if Ok
            Err(git_vfs::GitVfsError::AlreadyExists) => {
                // If it already exists, we need to retrieve its hash.
                // Re-calculate the hash based on the content, assuming it's consistent.
                let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                    author: node2_latest_commit.author.clone(),
                    message: node2_latest_commit.message.clone(),
                    tree_hash: node2_latest_commit.tree_hash.clone(),
                    parent_hashes: node2_latest_commit.parent_hashes.clone(),
                });
                node1_vfs.data_sha256(&commit_object.to_vec())
            },
            Err(e) => panic!("Failed to create commit: {:?}", e),
        };
        println!("Node 1 synced from Node 2 (object only).");

        // Node 3 syncs from Node 1 (if initialized)
        if new_nodes_initialized {
            let node1_latest_commit_hash = node1_vfs.get_ref(main_ref).unwrap();
            let node1_latest_commit = match node1_vfs.get_object(&node1_latest_commit_hash).unwrap() {
                git_vfs::GitObject::Commit(c) => c,
                _ => panic!("Expected commit object"),
            };

            // Fetch and create tree object
            let node1_tree = match node1_vfs.get_object(&node1_latest_commit.tree_hash).unwrap() {
                git_vfs::GitObject::Tree(t) => t,
                _ => panic!("Expected tree object"),
            };
            node3_vfs.create_tree(node1_tree.clone()).unwrap();

            // Fetch and create blob objects
            for (_, (blob_hash, _)) in node1_tree.iter() {
                let blob_data = match node1_vfs.get_object(blob_hash).unwrap() {
                    git_vfs::GitObject::Blob(b) => b,
                    _ => panic!("Expected blob object"),
                };
                node3_vfs.create_blob(&blob_data).unwrap();
            }

            // Create the commit object in Node 3
            let node3_sync_commit_hash = match node3_vfs.create_commit(
                &node1_latest_commit.author,
                &node1_latest_commit.message,
                &node1_latest_commit.tree_hash,
                node1_latest_commit.parent_hashes.clone(),
            ) {
                Ok(hash) => hash,
                Err(git_vfs::GitVfsError::AlreadyExists) => {
                    // If it already exists, we need to retrieve its hash.
                    // Re-calculate the hash based on the content, assuming it's consistent.
                    let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                        author: node1_latest_commit.author.clone(),
                        message: node1_latest_commit.message.clone(),
                        tree_hash: node1_latest_commit.tree_hash.clone(),
                        parent_hashes: node1_latest_commit.parent_hashes.clone(),
                    });
                    node3_vfs.data_sha256(&commit_object.to_vec())
                },
                Err(e) => panic!("Failed to create commit: {:?}", e),
            };

            node3_vfs.update_ref(main_ref, &node1_latest_commit_hash).unwrap(); // Node 3 updates to Node 1's latest
            println!("Node 3 synced from Node 1.");
        }

        // Node 4 syncs from Node 2 (if initialized)
        if new_nodes_initialized {
            let node2_latest_commit_hash = node2_vfs.get_ref(main_ref).unwrap();
            let node2_latest_commit = match node2_vfs.get_object(&node2_latest_commit_hash).unwrap() {
                git_vfs::GitObject::Commit(c) => c,
                _ => panic!("Expected commit object"),
            };

            // Fetch and create tree object
            let node2_tree = match node2_vfs.get_object(&node2_latest_commit.tree_hash).unwrap() {
                git_vfs::GitObject::Tree(t) => t,
                _ => panic!("Expected tree object"),
            };
            node4_vfs.create_tree(node2_tree.clone()).unwrap();

            // Fetch and create blob objects
            for (_, (blob_hash, _)) in node2_tree.iter() {
                let blob_data = match node2_vfs.get_object(blob_hash).unwrap() {
                    git_vfs::GitObject::Blob(b) => b,
                    _ => panic!("Expected blob object"),
                };
                node4_vfs.create_blob(&blob_data).unwrap();
            }

            // Create the commit object in Node 4
            let node4_sync_commit_hash = match node4_vfs.create_commit(
                &node2_latest_commit.author,
                &node2_latest_commit.message,
                &node2_latest_commit.tree_hash,
                node2_latest_commit.parent_hashes.clone(),
            ) {
                Ok(hash) => hash,
                Err(git_vfs::GitVfsError::AlreadyExists) => {
                    // If it already exists, we need to retrieve its hash.
                    // Re-calculate the hash based on the content, assuming it's consistent.
                    let commit_object = git_vfs::GitObject::Commit(git_vfs::Commit {
                        author: node2_latest_commit.author.clone(),
                        message: node2_latest_commit.message.clone(),
                        tree_hash: node2_latest_commit.tree_hash.clone(),
                        parent_hashes: node2_latest_commit.parent_hashes.clone(),
                    });
                    node4_vfs.data_sha256(&commit_object.to_vec())
                },
                Err(e) => panic!("Failed to create commit: {:?}", e),
            };

            node4_vfs.update_ref(main_ref, &node2_latest_commit_hash).unwrap(); // Node 4 updates to Node 2's latest
            println!("Node 4 synced from Node 2.");
        }


        println!("\n--- State after sync cycle {}", commit_counter);
        print_vfs_state(&node1_vfs, "Node 1 (Final)");
        print_vfs_state(&node2_vfs, "Node 2 (Final)");
        if new_nodes_initialized {
            print_vfs_state(&node3_vfs, "Node 3 (Final)");
            print_vfs_state(&node4_vfs, "Node 4 (Final)");
        }


        sleep(Duration::from_secs(3)).await;
        commit_counter += 1;
    }

    println!("\n--- Live Test Finished ---");
}
