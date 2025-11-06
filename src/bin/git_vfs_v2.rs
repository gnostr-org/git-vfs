// --- Required Imports ---
use std::io;
use std::env;

// Import the GitVfs struct and its related types from the library
use git_vfs::{GitVfs, GitVfsResult};

// The following libp2p related items are not directly used in this simple binary and can be removed for now.
// They are part of the network layer which is handled separately.
// use futures::{AsyncReadExt, AsyncWriteExt};
// use libp2p={
//     kad::{store::MemoryStore},
//     mdns,
//     request_response::{self},
//     swarm::{NetworkBehaviour},
//     PeerId,
// };
// use libp2p::kad::Behaviour as Kademlia;
// use libp2p::kad::Event as KademliaEvent;
// use async_trait::async_trait;
// use libp2p::StreamProtocol;
// use libp2p::identify::Behaviour as IdentifyBehaviour;


// Removed GitVfsProtocol, GitVfsBehaviour, GitVfsBehaviourEvent and their implementations
// as they are not needed for a simple binary demonstrating GitVfs core functionality.

#[tokio::main]
async fn main() {
    println!("Git VFS v2 Binary starting...");

    let mut git_vfs = GitVfs::new();

    // --- Demonstrate GitVfs functionality ---

    // 1. Create a blob (file content)
    let file_content = b"This is the content of my first file.";
    let blob_hash = git_vfs.create_blob(file_content).expect("Failed to create blob");
    println!("Created blob with hash: {}", blob_hash);

    // Verify blob content
    let retrieved_blob = git_vfs.get_object(&blob_hash).expect("Failed to retrieve blob");
    println!("Retrieved blob content: \"{}\"", String::from_utf8_lossy(&retrieved_blob));

    // 2. Create a reference (e.g., a branch)
    let branch_name = "refs/heads/main";
    let initial_commit_hash = &blob_hash; // For simplicity, the first commit points to the blob
    git_vfs.create_ref(branch_name, initial_commit_hash).expect("Failed to create ref");
    println!("Created ref '{}' pointing to: {}", branch_name, initial_commit_hash);

    // 3. Set HEAD to point to the main branch
    git_vfs.set_head(branch_name).expect("Failed to set HEAD");
    println!("Set HEAD to: {}", branch_name);

    // 4. Get HEAD and verify
    let current_head = git_vfs.get_head().expect("Failed to get HEAD");
    println!("Current HEAD: {}", current_head);

    // 5. Get the hash for the main branch ref
    let main_ref_hash = git_vfs.get_ref(branch_name).expect("Failed to get ref");
    println!("Hash for ref '{}': {}", branch_name, main_ref_hash);

    // 6. Update the main branch ref to a new commit hash (simulated)
    let new_commit_hash = "a_new_commit_hash_12345";
    git_vfs.update_ref(branch_name, new_commit_hash).expect("Failed to update ref");
    println!("Updated ref '{}' to: {}", branch_name, new_commit_hash);

    // Verify the update
    let updated_main_ref_hash = git_vfs.get_ref(branch_name).expect("Failed to get updated ref");
    println!("Verified updated ref '{}' hash: {}", branch_name, updated_main_ref_hash);

    // 7. Demonstrate SHA256 hashing
    let data_to_hash = b"This data will be hashed.";
    let sha256_hash = git_vfs.data_sha256(data_to_hash);
    println!("SHA256 hash of \"{}\": {}", String::from_utf8_lossy(data_to_hash), sha256_hash);

    println!("Git VFS v2 Binary finished demonstration.");
}
