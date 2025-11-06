use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p::{
    kad::{store::MemoryStore, Behaviour as Kademlia, Event as KademliaEvent},
    mdns,
    request_response::{self},
    swarm::{NetworkBehaviour},
    StreamProtocol,
    identify::Behaviour as IdentifyBehaviour,
};

#[derive(Debug, PartialEq)]
pub enum GitVfsError {
    NotFound,
    AlreadyExists,
    InvalidOperation,
}

pub type GitVfsResult<T> = Result<T, GitVfsError>;

// --- New definitions for Git objects ---
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub author: String,
    pub message: String,
    pub tree_hash: String, // Hash of the root tree for this commit
    pub parent_hashes: Vec<String>, // Hashes of parent commits
}

#[derive(Debug, Clone, PartialEq)]
pub enum GitObjectKind {
    Blob,
    Tree,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GitObject {
    Blob(Vec<u8>),
    Commit(Commit),
    Tree(HashMap<String, (String, GitObjectKind)>), // name -> (hash, kind)
}

impl GitObject {
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            GitObject::Blob(data) => data.clone(),
            GitObject::Commit(commit) => {
                let parents = commit.parent_hashes.join(" ");
                format!(
                    "mock_commit_author:{}\nmock_commit_message:{}\nmock_commit_tree:{}\nmock_commit_parents:{}",
                    commit.author, commit.message, commit.tree_hash, parents
                ).into_bytes()
            }
            GitObject::Tree(entries) => {
                let tree_object = GitObject::Tree(entries.clone());
                tree_object.to_vec()
            }
        }
    }
}
// --- End of new definitions ---

pub struct GitVfs {
    objects: HashMap<String, Vec<u8>>, // Stores git objects (blobs, trees, commits)
    refs: HashMap<String, String>,     // Stores references (branches, tags)
    head: Option<String>,              // Stores the current HEAD reference
}

impl Default for GitVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl GitVfs {
    pub fn new() -> Self {
        GitVfs {
            objects: HashMap::new(),
            refs: HashMap::new(),
            head: None,
        }
    }

    pub fn create_object(&mut self, hash: &str, data: &[u8]) -> GitVfsResult<()> {
        if self.objects.contains_key(hash) {
            return Err(GitVfsError::AlreadyExists);
        }
        self.objects.insert(hash.to_string(), data.to_vec());
        Ok(())
    }

    // Modified get_object to return GitObject and include mock deserialization
    pub fn get_object(&self, hash: &str) -> GitVfsResult<GitObject> {
        match self.objects.get(hash) {
            Some(data) => {
                let data_str = String::from_utf8_lossy(data);
                if data_str.contains("mock_commit_author") {
                    let mut author = "Unknown Author".to_string();
                    let mut message = "Unknown Message".to_string();
                    let mut tree_hash = String::new();
                    let mut parent_hashes = Vec::new();

                    for line in data_str.lines() {
                        if let Some(a) = line.strip_prefix("mock_commit_author:") {
                            author = a.trim().to_string();
                        } else if let Some(m) = line.strip_prefix("mock_commit_message:") {
                            message = m.trim().to_string();
                        } else if let Some(t) = line.strip_prefix("mock_commit_tree:") {
                            tree_hash = t.trim().to_string();
                        } else if let Some(p) = line.strip_prefix("mock_commit_parents:") {
                            parent_hashes = p.trim().split(' ').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
                        }
                    }
                    Ok(GitObject::Commit(Commit { author, message, tree_hash, parent_hashes }))
                } else if data_str.lines().any(|line| line.starts_with("blob ") || line.starts_with("tree ")) {
                    let mut entries = HashMap::new();
                    for line in data_str.lines() {
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() == 3 {
                            let kind = match parts[0] {
                                "blob" => GitObjectKind::Blob,
                                "tree" => GitObjectKind::Tree,
                                _ => continue,
                            };
                            entries.insert(parts[2].to_string(), (parts[1].to_string(), kind));
                        }
                    }
                    Ok(GitObject::Tree(entries))
                } else {
                    Ok(GitObject::Blob(data.clone()))
                }
            }
            None => Err(GitVfsError::NotFound),
        }
    }

    pub fn create_ref(&mut self, ref_name: &str, hash: &str) -> GitVfsResult<()> {
        self.refs.insert(ref_name.to_string(), hash.to_string());
        Ok(())
    }

    pub fn get_ref(&self, ref_name: &str) -> GitVfsResult<String> {
        match self.refs.get(ref_name) {
            Some(hash) => Ok(hash.clone()),
            None => Err(GitVfsError::NotFound),
        }
    }

    pub fn update_ref(&mut self, ref_name: &str, hash: &str) -> GitVfsResult<()> {
        if !self.refs.contains_key(ref_name) {
            return Err(GitVfsError::NotFound);
        }
        self.refs.insert(ref_name.to_string(), hash.to_string());
        Ok(())
    }

    pub fn set_head(&mut self, ref_name: &str) -> GitVfsResult<()> {
        if !self.refs.contains_key(ref_name) {
            return Err(GitVfsError::NotFound);
        }
        self.head = Some(ref_name.to_string());
        Ok(())
    }

    pub fn get_head(&self) -> GitVfsResult<String> {
        match &self.head {
            Some(head_ref) => Ok(head_ref.clone()),
            None => Err(GitVfsError::NotFound),
        }
    }

    pub fn create_blob(&mut self, data: &[u8]) -> GitVfsResult<String> {
        let hash = self.data_sha256(data);
        self.create_object(&hash, data)?;
        Ok(hash)
    }

    pub fn create_tree(&mut self, entries: HashMap<String, (String, GitObjectKind)>) -> GitVfsResult<String> {
        let tree_object = GitObject::Tree(entries);
        let data = tree_object.to_vec();
        let hash = self.data_sha256(&data);
        self.create_object(&hash, &data)?;
        Ok(hash)
    }

    pub fn get_tree(&self, hash: &str) -> GitVfsResult<HashMap<String, (String, GitObjectKind)>> {
        match self.get_object(hash)? {
            GitObject::Tree(entries) => Ok(entries),
            _ => Err(GitVfsError::InvalidOperation),
        }
    }

    pub fn create_commit(&mut self, author: &str, message: &str, tree_hash: &str, parent_hashes: Vec<String>) -> GitVfsResult<String> {
        // Ensure the tree object exists
        if let Err(GitVfsError::NotFound) = self.get_object(tree_hash) {
            return Err(GitVfsError::NotFound);
        }

        let commit_object = GitObject::Commit(Commit {
            author: author.to_string(),
            message: message.to_string(),
            tree_hash: tree_hash.to_string(),
            parent_hashes,
        });
        let data = commit_object.to_vec();
        let hash = self.data_sha256(&data);
        self.create_object(&hash, &data)?;
        Ok(hash)
    }

    pub fn list_refs(&self) -> HashMap<String, String> {
        self.refs.clone()
    }

    pub fn delete_ref(&mut self, ref_name: &str) -> GitVfsResult<()> {
        if self.refs.remove(ref_name).is_some() {
            Ok(())
        } else {
            Err(GitVfsError::NotFound)
        }
    }

    pub fn data_sha256(&self, data_to_hash: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data_to_hash);
        let result = hasher.finalize();

        hex::encode(result)
    }

    // --- Added walk_history method ---
    pub fn walk_history(&self, head_hash: &str) -> GitVfsResult<Vec<String>> {
        let mut history = Vec::new();
        let mut current_hash = Some(head_hash.to_string());

        while let Some(hash) = current_hash {
            match self.get_object(&hash)? {
                GitObject::Commit(commit) => {
                    history.push(hash.clone());
                    current_hash = commit.parent_hashes.first().cloned(); // Follow the first parent for simplicity
                },
                _ => return Err(GitVfsError::InvalidOperation), // Head hash must point to a commit
            }
        }
        Ok(history)
    }
    // --- End of added walk_history method ---
}

/// The libp2p protocol for requesting a Git object.
/// The Request is a `String` (the hash), the Response is a `Vec<u8>` (the raw object data).
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct GitVfsProtocol;

#[async_trait::async_trait]
impl libp2p::request_response::Codec for GitVfsProtocol {

    type Protocol = StreamProtocol;
    type Request = String;
    type Response = Vec<u8>;

    async fn read_request<TRs: AsyncReadExt + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut TRs,
    ) -> io::Result<Self::Request> {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn read_response<TRs: AsyncReadExt + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut TRs,
    ) -> io::Result<Self::Response> {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        Ok(buf)
    }

    async fn write_request<TWs: AsyncWriteExt + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut TWs,
        item: Self::Request,
    ) -> io::Result<()> {
        io.write_all(item.as_bytes()).await?;
        Ok(())
    }

    async fn write_response<TWs: AsyncWriteExt + Unpin + Send>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut TWs,
        item: Self::Response,
    ) -> io::Result<()> {
        io.write_all(&item).await?;
        Ok(())
    }
}

/// The combined libp2p NetworkBehaviour.
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "GitVfsBehaviourEvent")]
#[allow(dead_code)]
struct GitVfsBehaviour {
    /// Kademlia DHT for peer and content discovery.
    kad: Kademlia<MemoryStore>,
    /// mDNS for local peer discovery.
    mdns: mdns::tokio::Behaviour,
    /// Request-Response protocol for object transfer.
    request_response: request_response::Behaviour<GitVfsProtocol>,
    /// Identify protocol to learn about other peers.
    identify: IdentifyBehaviour,
}

// Boilerplate to map sub-behaviour events to the main event type
#[allow(dead_code)]
enum GitVfsBehaviourEvent {
    Kad(KademliaEvent),
    Mdns(mdns::Event),
    RequestResponse(request_response::Event<
        <GitVfsProtocol as libp2p::request_response::Codec>::Request,
        <GitVfsProtocol as libp2p::request_response::Codec>::Response,
    >),
    Identify(libp2p::identify::Event),
}
impl From<KademliaEvent> for GitVfsBehaviourEvent {
    fn from(v: KademliaEvent) -> Self { Self::Kad(v) }
}
impl From<mdns::Event> for GitVfsBehaviourEvent {
    fn from(v: mdns::Event) -> Self { Self::Mdns(v) }
}
impl From<request_response::Event<
    <GitVfsProtocol as libp2p::request_response::Codec>::Request,
    <GitVfsProtocol as libp2p::request_response::Codec>::Response,
>> for GitVfsBehaviourEvent {
    fn from(v: request_response::Event<
        <GitVfsProtocol as libp2p::request_response::Codec>::Request,
        <GitVfsProtocol as libp2p::request_response::Codec>::Response,
    >) -> Self { Self::RequestResponse(v) }
}
impl From<libp2p::identify::Event> for GitVfsBehaviourEvent {
    fn from(v: libp2p::identify::Event) -> Self { Self::Identify(v) }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use libp2p::identity::Keypair;
    use libp2p::PeerId;

    use super::*;
    use futures::io::Cursor;
    use libp2p::request_response::Codec;

    // Helper function to print the current state of a GitVfs instance
    fn print_vfs_state(vfs: &GitVfs, node_name: &str) {
        println!("--- {} VFS State ---", node_name);
        if let Some(head) = &vfs.head {
            println!("HEAD: {}", head);
            match vfs.get_ref(head) {
                Ok(hash) => println!("  -> Ref '{}' points to hash: {}", head, hash),
                Err(_) => println!("  -> Ref '{}' not found or invalid.", head),
            }
        } else {
            println!("HEAD: (not set)");
        }

        println!("Refs:");
        if vfs.refs.is_empty() {
            println!("  (no refs)");
        } else {
            for (ref_name, hash) in &vfs.refs {
                println!("  - {}: {}", ref_name, hash);
            }
        }
        println!("---------------------");
    }

    #[tokio::test]
    async fn test_read_write_request() {
        let mut codec = GitVfsProtocol;
        let protocol = StreamProtocol::new("/git-vfs/1.0.0");
        let request_data = "test_hash_123".to_string();
        let mut io_buffer = Cursor::new(Vec::new());

        // Write request
        codec.write_request(&protocol, &mut io_buffer, request_data.clone()).await.unwrap();

        // Reset cursor and read request
        io_buffer.set_position(0);
        let read_request = codec.read_request(&protocol, &mut io_buffer).await.unwrap();

        assert_eq!(read_request, request_data);
    }

    #[tokio::test]
    async fn test_read_write_response() {
        let mut codec = GitVfsProtocol;
        let protocol = StreamProtocol::new("/git-vfs/1.0.0");
        let response_data = vec![1, 2, 3, 4, 5];
        let mut io_buffer = Cursor::new(Vec::new());

        // Write response
        codec.write_response(&protocol, &mut io_buffer, response_data.clone()).await.unwrap();

        // Reset cursor and read response
        io_buffer.set_position(0);
        let read_response = codec.read_response(&protocol, &mut io_buffer).await.unwrap();

        assert_eq!(read_response, response_data);
    }

    #[test]
    fn test_behaviour_event_from_mdns_event() {
        let event = mdns::Event::Discovered(vec![(PeerId::random(), "/ip4/127.0.0.1/tcp/0".parse().unwrap())]);
        let behaviour_event: GitVfsBehaviourEvent = event.into();
        match behaviour_event {
            GitVfsBehaviourEvent::Mdns(_) => assert!(true),
            _ => panic!("Unexpected event type"),
        }
    }

    #[test]
    fn test_behaviour_event_from_identify_event() {
        let keypair = Keypair::generate_ed25519();
        let event = libp2p::identify::Event::Received {
            peer_id: PeerId::random(),
            info: libp2p::identify::Info {
                public_key: keypair.public(),
                listen_addrs: vec![],
                protocols: vec![],
                agent_version: "test-agent".to_string(),
                protocol_version: "test-protocol".to_string(),
                observed_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            },
        };
        let behaviour_event: GitVfsBehaviourEvent = event.into();
        match behaviour_event {
            GitVfsBehaviourEvent::Identify(_) => assert!(true),
            _ => panic!("Unexpected event type"),
        }
    }

    // --- GitVfs struct methods tests ---

    #[test]
    fn test_git_vfs_new() {
        let git_vfs = GitVfs::new();
        assert!(git_vfs.objects.is_empty());
        assert!(git_vfs.refs.is_empty());
        assert!(git_vfs.head.is_none());
    }

    #[test]
    fn test_create_object_and_get_object() {
        let mut git_vfs = GitVfs::new();
        let hash = "test_hash_123";
        let data = b"some data";

        let result = git_vfs.create_object(hash, data);
        assert!(result.is_ok());

        // Test get_object returning GitObject
        let retrieved_object = git_vfs.get_object(hash).unwrap();
        match retrieved_object {
            GitObject::Blob(retrieved_data) => assert_eq!(retrieved_data, data),
            _ => panic!("Expected Blob, got something else"),
        }
    }

    #[test]
    fn test_create_object_already_exists() {
        let mut git_vfs = GitVfs::new();
        let hash = "test_hash_123";
        let data = b"some data";

        git_vfs.create_object(hash, data).unwrap();
        let result = git_vfs.create_object(hash, b"different data");
        assert_eq!(result, Err(GitVfsError::AlreadyExists));
    }

    #[test]
    fn test_get_object_not_found() {
        let git_vfs = GitVfs::new();
        let result = git_vfs.get_object("non_existent_hash");
        assert_eq!(result, Err(GitVfsError::NotFound));
    }

    #[test]
    fn test_create_ref_and_get_ref() {
        let mut git_vfs = GitVfs::new();
        let ref_name = "refs/heads/main";
        let hash = "main_commit_hash";

        let result = git_vfs.create_ref(ref_name, hash);
        assert!(result.is_ok());

        let retrieved_hash = git_vfs.get_ref(ref_name).unwrap();
        assert_eq!(retrieved_hash, hash);
    }

    #[test]
    fn test_get_ref_not_found() {
        let git_vfs = GitVfs::new();
        let result = git_vfs.get_ref("refs/heads/non_existent");
        assert_eq!(result, Err(GitVfsError::NotFound));
    }

    #[test]
    fn test_update_ref() {
        let mut git_vfs = GitVfs::new();
        let ref_name = "refs/heads/main";
        let initial_hash = "initial_hash";
        let new_hash = "new_hash";

        git_vfs.create_ref(ref_name, initial_hash).unwrap();
        let result = git_vfs.update_ref(ref_name, new_hash);
        assert!(result.is_ok());

        let retrieved_hash = git_vfs.get_ref(ref_name).unwrap();
        assert_eq!(retrieved_hash, new_hash);
    }

    #[test]
    fn test_update_ref_not_found() {
        let mut git_vfs = GitVfs::new();
        let result = git_vfs.update_ref("refs/heads/non_existent", "some_hash");
        assert_eq!(result, Err(GitVfsError::NotFound));
    }

    #[test]
    fn test_set_head_and_get_head() {
        let mut git_vfs = GitVfs::new();
        let ref_name = "refs/heads/main";
        let hash = "main_commit_hash";

        git_vfs.create_ref(ref_name, hash).unwrap();
        let result = git_vfs.set_head(ref_name);
        assert!(result.is_ok());

        let head_ref = git_vfs.get_head().unwrap();
        assert_eq!(head_ref, ref_name);
    }

    #[test]
    fn test_set_head_ref_not_found() {
        let mut git_vfs = GitVfs::new();
        let result = git_vfs.set_head("refs/heads/non_existent");
        assert_eq!(result, Err(GitVfsError::NotFound));
    }

    #[test]
    fn test_get_head_when_none_set() {
        let git_vfs = GitVfs::new();
        let result = git_vfs.get_head();
        assert_eq!(result, Err(GitVfsError::NotFound));
    }

    #[test]
    fn test_create_blob() {
        let mut git_vfs = GitVfs::new();
        let data = b"blob content";
        // SHA256 hash for b"blob content"
        let expected_hash = "7b24cf3d897fd680e0258c1c7c23db50a5428581ed1785c08de505c381b4c4b5";

        let hash = git_vfs.create_blob(data).unwrap();
        assert_eq!(hash, expected_hash);

        let retrieved_object = git_vfs.get_object(&hash).unwrap();
        match retrieved_object {
            GitObject::Blob(retrieved_data) => assert_eq!(retrieved_data, data),
            _ => panic!("Expected Blob, got something else"),
        }
    }

    #[test]
    fn test_create_blob_empty_data() {
        let mut git_vfs = GitVfs::new();
        let data = b"";
        // SHA256 hash for b""
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

        let hash = git_vfs.create_blob(data).unwrap();
        assert_eq!(hash, expected_hash);

        let retrieved_object = git_vfs.get_object(&hash).unwrap();
        match retrieved_object {
            GitObject::Blob(retrieved_data) => assert_eq!(retrieved_data, data),
            _ => panic!("Expected Blob, got something else"),
        }
    }

    #[test]
    fn test_data_sha256_byte_slice() {
        let git_vfs = GitVfs::new();
        let data: &[u8] = b"test data";
        let expected_hash = "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9";
        let actual_hash = git_vfs.data_sha256(data);
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_data_sha256_string() {
        let git_vfs = GitVfs::new();
        let data = String::from("hello world");
        let expected_hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let actual_hash = git_vfs.data_sha256(data.as_bytes());
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_data_sha256_empty_data() {
        let git_vfs = GitVfs::new();
        let data: &[u8] = b"";
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let actual_hash = git_vfs.data_sha256(data);
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_data_sha256_multiple_updates() {
        let _git_vfs = GitVfs::new();
        let mut hasher = Sha256::new();
        hasher.update(b"part one ");
        hasher.update(b"part two");
        let combined_hash = hex::encode(hasher.finalize());
        let single_hash = hex::encode(Sha256::digest(b"part one part two"));
        assert_eq!(combined_hash, single_hash);
    }

    // --- Test for simulating peer synchronization ---
    #[tokio::test]
    async fn test_peer_sync() {
        let mut vfs_server = GitVfs::new();
        let mut vfs_client = GitVfs::new();

        // --- Server: Populate initial state ---
        println!("--- {} Initializing ---", "Server");
        // Use mock commit data for testing get_object's deserialization
        let mock_commit_data_server = b"mock_commit_author:Server
mock_commit_message:Initial commit from server";
        let blob_hash_server = vfs_server.data_sha256(mock_commit_data_server); // Use hash of the mock data
        vfs_server.create_object(&blob_hash_server, mock_commit_data_server).expect("Server: Failed to create mock commit object");

        let ref_name_server = "refs/heads/main";
        vfs_server.create_ref(ref_name_server, &blob_hash_server).expect("Server: Failed to create ref");
        vfs_server.set_head(ref_name_server).expect("Server: Failed to set HEAD");
        print_vfs_state(&vfs_server, "Server");

        // --- Client: Fetch changes from server ---
        println!("--- {} Fetching from Server ---", "Client");

        // Fetch object
        let server_object_data = vfs_server.get_object(&blob_hash_server).expect("Server: Failed to get object for client");
        // We need to store the raw data to create the object in the client.
        // The get_object in lib.rs now returns GitObject, so we need to extract the raw data.
        let raw_data_to_store = match server_object_data {
            GitObject::Commit(c) => {
                // Reconstruct the mock data format expected by get_object for deserialization
                format!("mock_commit_author:{}\nmock_commit_message:{}", c.author, c.message).into_bytes()
            },
            GitObject::Blob(d) => d,
            GitObject::Tree(_) => panic!("Expected Blob or Commit, got Tree"),
        };
        vfs_client.create_object(&blob_hash_server, &raw_data_to_store).expect("Client: Failed to create object");
        println!("Client: Created object for hash {}", blob_hash_server);

        // Fetch ref
        let server_ref_hash = vfs_server.get_ref(ref_name_server).expect("Server: Failed to get ref for client");
        vfs_client.create_ref(ref_name_server, &server_ref_hash).expect("Client: Failed to create ref");
        println!("Client: Created ref '{}' pointing to {}", ref_name_server, server_ref_hash);

        // Fetch HEAD
        let server_head_ref = vfs_server.get_head().expect("Server: Failed to get HEAD for client");
        vfs_client.set_head(&server_head_ref).expect("Client: Failed to set HEAD");
        println!("Client: Set HEAD to {}", server_head_ref);

        print_vfs_state(&vfs_client, "Client");

        // --- Server: Populate new changes ---
        println!("--- {} Making new changes ---", "Server");
        let new_mock_commit_data_server = b"mock_commit_author:Server
mock_commit_message:New content from server";
        let new_blob_hash_server = vfs_server.data_sha256(new_mock_commit_data_server);
        vfs_server.create_object(&new_blob_hash_server, new_mock_commit_data_server).expect("Server: Failed to create new mock commit object");
        println!("Server: Created new object with hash {}", new_blob_hash_server);
        vfs_server.update_ref(ref_name_server, &new_blob_hash_server).expect("Server: Failed to update ref");
        println!("Server: Updated ref '{}' to {}", ref_name_server, new_blob_hash_server);
        print_vfs_state(&vfs_server, "Server");

        // --- Client: Fetch updated changes from server ---
        println!("--- {} Fetching updated changes from Server ---", "Client");

        // Fetch updated object
        let server_new_object_data = vfs_server.get_object(&new_blob_hash_server).expect("Server: Failed to get new object for client");
        let raw_data_to_store_new = match server_new_object_data {
            GitObject::Commit(c) => {
                format!("mock_commit_author:{}\nmock_commit_message:{}", c.author, c.message).into_bytes()
            },
            GitObject::Blob(d) => d,
            GitObject::Tree(t) => GitObject::Tree(t).to_vec(),
        };
        vfs_client.create_object(&new_blob_hash_server, &raw_data_to_store_new).expect("Client: Failed to create new object");
        println!("Client: Created new object for hash {}", new_blob_hash_server);

        // Fetch updated ref
        let server_updated_ref_hash = vfs_server.get_ref(ref_name_server).expect("Server: Failed to get updated ref for client");
        vfs_client.update_ref(ref_name_server, &server_updated_ref_hash).expect("Client: Failed to update ref");
        println!("Client: Updated ref '{}' to {}", ref_name_server, server_updated_ref_hash);

        print_vfs_state(&vfs_client, "Client");

        // --- Now, reverse the roles: Client becomes server, Server becomes client ---
        println!("--- {} Reversing roles: Original Client becomes New Server ---", "");
        let mut vfs_server_new = GitVfs::new(); // This will be the new server
        let mut vfs_client_new = GitVfs::new(); // This will be the new client

        // Populate new server state (using original client's state as source)
        println!("--- {} Initializing ---", "New Server (Original Client)");
        let mock_commit_data_client_orig = b"mock_commit_author:OriginalClient
mock_commit_message:Content from original client";
        let blob_hash_client_orig = vfs_client.data_sha256(mock_commit_data_client_orig);
        vfs_client.create_object(&blob_hash_client_orig, mock_commit_data_client_orig).expect("Original Client: Failed to create mock commit object");
        println!("Original Client: Created mock commit object with hash {}", blob_hash_client_orig);
        let ref_name_client_orig = "refs/heads/feature";
        vfs_client.create_ref(ref_name_client_orig, &blob_hash_client_orig).expect("Original Client: Failed to create ref");
        println!("Original Client: Created ref '{}' pointing to {}", ref_name_client_orig, blob_hash_client_orig);
        vfs_client.set_head(ref_name_client_orig).expect("Original Client: Failed to set HEAD");
        println!("Original Client: Set HEAD to {}", ref_name_client_orig);
        print_vfs_state(&vfs_client, "Original Client");

        // --- New Client: Fetch changes from original client ---
        println!("--- {} Fetching from Original Client ---", "New Client");
        let client_orig_object_data = vfs_client.get_object(&blob_hash_client_orig).expect("Original Client: Failed to get object for new client");
        let raw_data_to_store_new_client = match client_orig_object_data {
            GitObject::Commit(c) => {
                format!("mock_commit_author:{}\nmock_commit_message:{}", c.author, c.message).into_bytes()
            },
            GitObject::Blob(d) => d,
            GitObject::Tree(t) => GitObject::Tree(t).to_vec(),
        };
        vfs_server_new.create_object(&blob_hash_client_orig, &raw_data_to_store_new_client).expect("New Server: Failed to create object");
        println!("New Server: Created object for hash {}", blob_hash_client_orig);

        let client_orig_ref_hash = vfs_client.get_ref(ref_name_client_orig).expect("Original Client: Failed to get ref for new client");
        vfs_server_new.create_ref(ref_name_client_orig, &client_orig_ref_hash).expect("New Server: Failed to create ref");
        println!("New Server: Created ref '{}' pointing to {}", ref_name_client_orig, client_orig_ref_hash);

        let client_orig_head_ref = vfs_client.get_head().expect("Original Client: Failed to get HEAD for new client");
        vfs_server_new.set_head(&client_orig_head_ref).expect("New Server: Failed to set HEAD");
        println!("New Server: Set HEAD to {}", client_orig_head_ref);

        print_vfs_state(&vfs_server_new, "New Server");

        // --- Verify new server state matches original client state ---
        assert_eq!(vfs_server_new.get_object(&blob_hash_client_orig).unwrap(), GitObject::Blob(b"Content from original client".to_vec()));
        assert_eq!(vfs_server_new.get_ref(ref_name_client_orig).unwrap(), client_orig_ref_hash);
        assert_eq!(vfs_server_new.get_head().unwrap(), client_orig_head_ref);
        println!("--- {} Verification successful: New Server state matches Original Client state ---", "");
    }
}
