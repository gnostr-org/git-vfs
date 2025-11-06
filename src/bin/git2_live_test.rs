

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

    // Create a ref for the main branch
    let main_ref = "refs/heads/main";
    node1_vfs.create_ref(main_ref, &blob1_hash).expect("Node 1: Failed to create main ref");
    node1_vfs.set_head(main_ref).expect("Node 1: Failed to set HEAD");
    print_vfs_state(&node1_vfs, "Node 1");

    // Create a second commit
    let file2_content = b"Second commit from Node 1.";
    let blob2_hash = node1_vfs.create_blob(file2_content).expect("Node 1: Failed to create blob 2");
    node1_vfs.update_ref(main_ref, &blob2_hash).expect("Node 1: Failed to update main ref");
    println!("\n--- Node 1: Made a new commit ---");
    print_vfs_state(&node1_vfs, "Node 1");

    // --- NODE 2: Initializing and Cloning from Node 1 ---
    println!("\n--- Node 2: Initializing and Cloning from Node 1 ---");
    let mut node2_vfs = GitVfs::new();

    // Simulate fetching the HEAD ref from Node 1
    let node1_head_ref = node1_vfs.get_head().expect("Node 1: Failed to get HEAD");
    let node1_main_hash = node1_vfs.get_ref(&node1_head_ref).expect("Node 1: Failed to get main ref hash");

    // Simulate fetching the object for the main branch hash
    let object_data = node1_vfs.get_object(&node1_main_hash).expect("Node 1: Failed to get object");
    let raw_data_for_create_object = match object_data {
        GitObject::Blob(data) => data.clone(),
        GitObject::Commit(commit) => {
            format!(
                "mock_commit_author:{}\nmock_commit_message:{}",
                commit.author, commit.message
            )
            .into_bytes()
        }
    };

    // Node 2 creates the object and ref
    node2_vfs.create_object(&node1_main_hash, &raw_data_for_create_object).expect("Node 2: Failed to create object");
    node2_vfs.create_ref(main_ref, &node1_main_hash).expect("Node 2: Failed to create main ref");
    node2_vfs.set_head(main_ref).expect("Node 2: Failed to set HEAD");

    println!("Node 2: Cloned main branch from Node 1.");
    print_vfs_state(&node2_vfs, "Node 2");

    // --- NODE 2: Creating its own commit history ---
    println!("\n--- Node 2: Creating a new commit on a feature branch ---");
    let feature_ref = "refs/heads/feature";
    let feature_content = b"Feature development on Node 2.";
    let feature_blob_hash = node2_vfs.create_blob(feature_content).expect("Node 2: Failed to create feature blob");
    node2_vfs.create_ref(feature_ref, &feature_blob_hash).expect("Node 2: Failed to create feature ref");
    node2_vfs.set_head(feature_ref).expect("Node 2: Failed to set HEAD to feature branch");
    print_vfs_state(&node2_vfs, "Node 2");

    // --- NODE 1: Fetching changes from Node 2 ---
    println!("\n--- Node 1: Fetching changes from Node 2 ---");

    // Simulate fetching the feature ref from Node 2
    let node2_feature_hash = node2_vfs.get_ref(feature_ref).expect("Node 2: Failed to get feature ref hash");
    let feature_object_data = node2_vfs.get_object(&node2_feature_hash).expect("Node 2: Failed to get feature object");

    // Node 1 creates the object and ref for the feature branch
    node1_vfs.create_object(&node2_feature_hash, &feature_object_data).expect("Node 1: Failed to create feature object");
    node1_vfs.create_ref(feature_ref, &node2_feature_hash).expect("Node 1: Failed to create feature ref");

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
