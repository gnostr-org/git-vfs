use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p::{
    kad::{store::MemoryStore, Behaviour as Kademlia, Event as KademliaEvent},
    mdns,
    request_response::{self},
    swarm::{NetworkBehaviour},
    PeerId,
    StreamProtocol,
    identify::Behaviour as IdentifyBehaviour,
};
use async_trait::async_trait;

#[derive(Debug, PartialEq)]
pub enum GitVfsError {
    NotFound,
    AlreadyExists,
    InvalidOperation,
}

pub type GitVfsResult<T> = Result<T, GitVfsError>;

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

    pub fn get_object(&self, hash: &str) -> GitVfsResult<Vec<u8>> {
        match self.objects.get(hash) {
            Some(data) => Ok(data.clone()),
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
        // For simplicity, we'll use the length as a placeholder hash for blobs.
        // In a real Git implementation, this would be a SHA-1 or SHA-256 hash.
        let hash = format!("{}", data.len());
        self.create_object(&hash, data)?;
        Ok(hash)
    }

    pub fn data_sha256(&mut self, data_to_hash: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data_to_hash);
        let result = hasher.finalize();

        hex::encode(result)
    }
}

/// The libp2p protocol for requesting a Git object.
/// The Request is a `String` (the hash), the Response is a `Vec<u8>` (the raw object data).
#[derive(Debug, Clone, Default)]
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

#[cfg(test)]
mod tests {
    use libp2p::identity::Keypair;

    use super::*;
    use futures::io::Cursor;
    use libp2p::request_response::Codec;

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

    // --- New Tests for GitVfs struct methods ---

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

        let retrieved_data = git_vfs.get_object(hash).unwrap();
        assert_eq!(retrieved_data, data);
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
        let expected_hash = format!("{}", data.len()); // Using length as hash for simplicity

        let hash = git_vfs.create_blob(data).unwrap();
        assert_eq!(hash, expected_hash);

        let retrieved_data = git_vfs.get_object(&hash).unwrap();
        assert_eq!(retrieved_data, data);
    }

    #[test]
    fn test_create_blob_empty_data() {
        let mut git_vfs = GitVfs::new();
        let data = b"";
        let expected_hash = "0"; // Length is 0

        let hash = git_vfs.create_blob(data).unwrap();
        assert_eq!(hash, expected_hash);

        let retrieved_data = git_vfs.get_object(&hash).unwrap();
        assert_eq!(retrieved_data, data);
    }

    #[test]
    fn test_data_sha256_byte_slice() {
        let mut git_vfs = GitVfs::new();
        let data: &[u8] = b"test data";
        let expected_hash = "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9";
        let actual_hash = git_vfs.data_sha256(data);
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_data_sha256_string() {
        let mut git_vfs = GitVfs::new();
        let data = String::from("hello world");
        let expected_hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let actual_hash = git_vfs.data_sha256(data.as_bytes());
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_data_sha256_empty_data() {
        let mut git_vfs = GitVfs::new();
        let data: &[u8] = b"";
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let actual_hash = git_vfs.data_sha256(data);
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_data_sha256_multiple_updates() {
        let mut git_vfs = GitVfs::new();
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
        let file_content_server = b"Content from server";
        let blob_hash_server = vfs_server.create_blob(file_content_server).expect("Server: Failed to create blob");
        let ref_name_server = "refs/heads/main";
        vfs_server.create_ref(ref_name_server, &blob_hash_server).expect("Server: Failed to create ref");
        vfs_server.set_head(ref_name_server).expect("Server: Failed to set HEAD");

        // --- Client: Fetch changes from server ---

        // Fetch object
        let server_object_data = vfs_server.get_object(&blob_hash_server).expect("Server: Failed to get object for client");
        vfs_client.create_object(&blob_hash_server, &server_object_data).expect("Client: Failed to create object");

        // Fetch ref
        let server_ref_hash = vfs_server.get_ref(ref_name_server).expect("Server: Failed to get ref for client");
        vfs_client.create_ref(ref_name_server, &server_ref_hash).expect("Client: Failed to create ref");

        // Fetch HEAD
        let server_head_ref = vfs_server.get_head().expect("Server: Failed to get HEAD for client");
        vfs_client.set_head(&server_head_ref).expect("Client: Failed to set HEAD");

        // --- Verify client state matches server state ---
        assert_eq!(vfs_client.get_object(&blob_hash_server).unwrap(), server_object_data);
        assert_eq!(vfs_client.get_ref(ref_name_server).unwrap(), server_ref_hash);
        assert_eq!(vfs_client.get_head().unwrap(), server_head_ref);

        // --- Server: Populate new changes ---
        let new_file_content_server = b"New content from server";
        let new_blob_hash_server = vfs_server.create_blob(new_file_content_server).expect("Server: Failed to create new blob");
        vfs_server.update_ref(ref_name_server, &new_blob_hash_server).expect("Server: Failed to update ref");
        // HEAD remains on main, so it implicitly points to the new commit.

        // --- Client: Fetch updated changes from server ---

        // Fetch updated object
        let server_new_object_data = vfs_server.get_object(&new_blob_hash_server).expect("Server: Failed to get new object for client");
        vfs_client.create_object(&new_blob_hash_server, &server_new_object_data).expect("Client: Failed to create new object");

        // Fetch updated ref
        let server_updated_ref_hash = vfs_server.get_ref(ref_name_server).expect("Server: Failed to get updated ref for client");
        vfs_client.update_ref(ref_name_server, &server_updated_ref_hash).expect("Client: Failed to update ref");

        // --- Verify client state matches updated server state ---
        assert_eq!(vfs_client.get_object(&new_blob_hash_server).unwrap(), server_new_object_data);
        assert_eq!(vfs_client.get_ref(ref_name_server).unwrap(), server_updated_ref_hash);
        // HEAD in client should still point to the old ref name, but the ref itself is updated.
        // If HEAD was pointing to a specific commit hash, we'd need to update that too.
        // For now, we assume HEAD points to a ref name, and the ref name is updated.
        assert_eq!(vfs_client.get_head().unwrap(), ref_name_server); // HEAD ref name is the same

        // --- Now, reverse the roles: Client becomes server, Server becomes client ---
        let mut vfs_server_new = GitVfs::new(); // This will be the new server
        let mut vfs_client_new = GitVfs::new(); // This will be the new client

        // Populate new server state (using original client's state as source)
        let file_content_client_orig = b"Content from original client";
        let blob_hash_client_orig = vfs_client.create_blob(file_content_client_orig).expect("Original Client: Failed to create blob");
        let ref_name_client_orig = "refs/heads/feature";
        vfs_client.create_ref(ref_name_client_orig, &blob_hash_client_orig).expect("Original Client: Failed to create ref");
        vfs_client.set_head(ref_name_client_orig).expect("Original Client: Failed to set HEAD");

        // --- New Client: Fetch changes from original client ---
        let client_orig_object_data = vfs_client.get_object(&blob_hash_client_orig).expect("Original Client: Failed to get object for new client");
        vfs_server_new.create_object(&blob_hash_client_orig, &client_orig_object_data).expect("New Server: Failed to create object");

        let client_orig_ref_hash = vfs_client.get_ref(ref_name_client_orig).expect("Original Client: Failed to get ref for new client");
        vfs_server_new.create_ref(ref_name_client_orig, &client_orig_ref_hash).expect("New Server: Failed to create ref");

        let client_orig_head_ref = vfs_client.get_head().expect("Original Client: Failed to get HEAD for new client");
        vfs_server_new.set_head(&client_orig_head_ref).expect("New Server: Failed to set HEAD");

        // --- Verify new server state matches original client state ---
        assert_eq!(vfs_server_new.get_object(&blob_hash_client_orig).unwrap(), client_orig_object_data);
        assert_eq!(vfs_server_new.get_ref(ref_name_client_orig).unwrap(), client_orig_ref_hash);
        assert_eq!(vfs_server_new.get_head().unwrap(), client_orig_head_ref);
    }
}