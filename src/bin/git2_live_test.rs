

// --- IMPORTS ---
use git_vfs::GitVfs;
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

    // The internal refs map is not public, so we cannot print it directly.
    // We would need to add a method to GitVfs to list refs if needed.
    println!("---------------------");
}

// --- MAIN TEST FUNCTION ---

#[tokio::main]
async fn main() {
    println!("--- Git VFS Live Test: 2-Node Git History Exchange ---");

    // --- NODE 1: Initializing and Creating History ---
    println!("\n--- Node 1: Initializing ---");
    let mut node1_vfs = GitVfs::new();

    // Create first commit (blob)
    let file1_content = b"Hello from Node 1!";
    let blob1_hash = node1_vfs.create_blob(file1_content).expect("Node 1: Failed to create blob 1");
    println!("Node 1: Created blob 1 with hash: {}", blob1_hash);

    let mut tree1_entries = std::collections::HashMap::new();
    tree1_entries.insert("file1.txt".to_string(), (blob1_hash.clone(), git_vfs::GitObjectKind::Blob));
    let tree1_hash = node1_vfs.create_tree(tree1_entries).unwrap();

    let commit1_hash = node1_vfs.create_commit(
        "Node 1 Author",
        "Initial commit from Node 1",
        &tree1_hash,
        vec![],
    ).unwrap();

    // Create a ref for the main branch
    let main_ref = "refs/heads/main";
    node1_vfs.create_ref(main_ref, &commit1_hash).expect("Node 1: Failed to create main ref");
    node1_vfs.set_head(main_ref).expect("Node 1: Failed to set HEAD");
    print_vfs_state(&node1_vfs, "Node 1");

    // Create a second commit
    let file2_content = b"Second commit from Node 1.";
    let blob2_hash = node1_vfs.create_blob(file2_content).expect("Node 1: Failed to create blob 2");

    let mut tree2_entries = node1_vfs.get_tree(&tree1_hash).unwrap();
    tree2_entries.insert("file2.txt".to_string(), (blob2_hash.clone(), git_vfs::GitObjectKind::Blob));
    let tree2_hash = node1_vfs.create_tree(tree2_entries).unwrap();

    let commit2_hash = node1_vfs.create_commit(
        "Node 1 Author",
        "Second commit from Node 1",
        &tree2_hash,
        vec![commit1_hash.clone()],
    ).unwrap();

    node1_vfs.update_ref(main_ref, &commit2_hash).expect("Node 1: Failed to update main ref");
    println!("\n--- Node 1: Made a new commit ---");
    print_vfs_state(&node1_vfs, "Node 1");

    // --- NODE 2: Initializing and Cloning from Node 1 ---
    println!("\n--- Node 2: Initializing and Cloning from Node 1 ---");
    let mut node2_vfs = GitVfs::new();

    // Simulate fetching the HEAD ref from Node 1
    let node1_head_commit_hash = node1_vfs.get_ref(main_ref).expect("Node 1: Failed to get main ref hash");
    let node1_head_commit = match node1_vfs.get_object(&node1_head_commit_hash).unwrap() {
        git_vfs::GitObject::Commit(c) => c,
        _ => panic!("Expected commit object"),
    };

    // Fetch and create tree object
    let node1_tree = match node1_vfs.get_object(&node1_head_commit.tree_hash).unwrap() {
        git_vfs::GitObject::Tree(t) => t,
        _ => panic!("Expected tree object"),
    };
    node2_vfs.create_tree(node1_tree.clone()).unwrap();

    // Fetch and create blob objects
    for (_, (blob_hash, _)) in node1_tree.iter() {
        let blob_data = match node1_vfs.get_object(blob_hash).unwrap() {
            git_vfs::GitObject::Blob(b) => b,
            _ => panic!("Expected blob object"),
        };
        node2_vfs.create_blob(&blob_data).unwrap();
    }

    // Create the commit object in Node 2
    node2_vfs.create_commit(
        &node1_head_commit.author,
        &node1_head_commit.message,
        &node1_head_commit.tree_hash,
        node1_head_commit.parent_hashes.clone(),
    ).unwrap();

    node2_vfs.create_ref(main_ref, &node1_head_commit_hash).expect("Node 2: Failed to create main ref");
    node2_vfs.set_head(main_ref).expect("Node 2: Failed to set HEAD");

    println!("Node 2: Cloned main branch from Node 1.");
    print_vfs_state(&node2_vfs, "Node 2");

    // --- NODE 2: Creating its own commit history ---
    println!("\n--- Node 2: Creating a new commit on a feature branch ---");
    let feature_ref = "refs/heads/feature";

    let node2_main_commit_hash = node2_vfs.get_ref(main_ref).unwrap();
    let node2_main_commit = match node2_vfs.get_object(&node2_main_commit_hash).unwrap() {
        git_vfs::GitObject::Commit(c) => c,
        _ => panic!("Expected commit object"),
    };
    let node2_main_tree = node2_vfs.get_tree(&node2_main_commit.tree_hash).unwrap();

    let feature_content = b"Feature development on Node 2.";
    let feature_blob_hash = node2_vfs.create_blob(feature_content).expect("Node 2: Failed to create feature blob");

    let mut feature_tree_entries = node2_main_tree.clone();
    feature_tree_entries.insert("feature_file.txt".to_string(), (feature_blob_hash.clone(), git_vfs::GitObjectKind::Blob));
    let feature_tree_hash = node2_vfs.create_tree(feature_tree_entries).unwrap();

    let feature_commit_hash = node2_vfs.create_commit(
        "Node 2 Author",
        "Feature commit on Node 2",
        &feature_tree_hash,
        vec![node2_main_commit_hash.clone()],
    ).unwrap();

    node2_vfs.create_ref(feature_ref, &feature_commit_hash).expect("Node 2: Failed to create feature ref");
    node2_vfs.set_head(feature_ref).expect("Node 2: Failed to set HEAD to feature branch");
    print_vfs_state(&node2_vfs, "Node 2");

    // --- NODE 1: Fetching changes from Node 2 ---
    println!("\n--- Node 1: Fetching changes from Node 2 ---");

    // Simulate fetching the feature ref from Node 2
    let node2_feature_commit_hash = node2_vfs.get_ref(feature_ref).expect("Node 2: Failed to get feature ref hash");
    let node2_feature_commit = match node2_vfs.get_object(&node2_feature_commit_hash).unwrap() {
        git_vfs::GitObject::Commit(c) => c,
        _ => panic!("Expected commit object"),
    };

    // Fetch and create tree object
    let node2_feature_tree = match node2_vfs.get_object(&node2_feature_commit.tree_hash).unwrap() {
        git_vfs::GitObject::Tree(t) => t,
        _ => panic!("Expected tree object"),
    };
    node1_vfs.create_tree(node2_feature_tree.clone()).unwrap();

    // Fetch and create blob objects
    for (_, (blob_hash, _)) in node2_feature_tree.iter() {
        let blob_data = match node2_vfs.get_object(blob_hash).unwrap() {
            git_vfs::GitObject::Blob(b) => b,
            _ => panic!("Expected blob object"),
        };
        node1_vfs.create_blob(&blob_data).unwrap();
    }

    // Create the commit object in Node 1
    node1_vfs.create_commit(
        &node2_feature_commit.author,
        &node2_feature_commit.message,
        &node2_feature_commit.tree_hash,
        node2_feature_commit.parent_hashes.clone(),
    ).unwrap();

    node1_vfs.create_ref(feature_ref, &node2_feature_commit_hash).expect("Node 1: Failed to create feature ref");

    println!("Node 1: Fetched feature branch from Node 2.");
    // We can't easily show all refs, but we can check if the ref exists
    assert!(node1_vfs.get_ref(feature_ref).is_ok());
    println!("Node 1 now has the feature branch ref.");


    // --- FINAL STATE ---
    println!("\n--- Final VFS States ---");
    print_vfs_state(&node1_vfs, "Node 1 (Final)");
    print_vfs_state(&node2_vfs, "Node 2 (Final)");
    println!("\n--- Live Test Finished ---");

    sleep(Duration::from_secs(2)).await;
}
