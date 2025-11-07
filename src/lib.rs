use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p::{
    StreamProtocol,
    identify::Behaviour as IdentifyBehaviour,
    kad::{Behaviour as Kademlia, Event as KademliaEvent, store::MemoryStore},
    mdns,
    request_response::{self},
    swarm::NetworkBehaviour,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;

pub mod altroot_vfs;
pub mod memory_vfs;
pub mod overlay_vfs;
pub mod embedded_vfs;

#[derive(Debug, PartialEq)]
pub enum GitVfsError {
    NotFound,
    AlreadyExists,
    InvalidOperation,
}

pub type GitVfsResult<T> = Result<T, GitVfsError>;

impl From<git2::Error> for GitVfsError {
    fn from(_err: git2::Error) -> Self {
        // For simplicity, mapping all git2 errors to InvalidOperation
        // In a real application, you might want more granular error handling
        GitVfsError::InvalidOperation
    }
}

impl From<GitVfsError> for vfs::VfsError {
    fn from(err: GitVfsError) -> Self {
        match err {
            GitVfsError::NotFound => vfs::VfsError::FileNotFound,
            GitVfsError::AlreadyExists => vfs::VfsError::FileExists,
            GitVfsError::InvalidOperation => vfs::VfsError::Other(Box::new(err)),
        }
    }
}

pub struct GitVfs {
    objects: HashMap<String, Vec<u8>>, // Stores git objects (blobs, trees, commits)
    refs: HashMap<String, String>,     // Stores references (branches, tags)
    head: Option<String>,              // Stores the current HEAD reference
}

#[derive(Debug, PartialEq)]
pub struct Diff {
    pub head_changed: Option<(Option<String>, Option<String>)>, // (old_head, new_head)
    pub refs_added: HashMap<String, String>,
    pub refs_removed: HashMap<String, String>,
    pub refs_updated: HashMap<String, (String, String)>, // (ref_name, (old_hash, new_hash))
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

    pub fn diff(&self, other: &GitVfs) -> Diff {
        let mut head_changed = None;
        if self.head != other.head {
            head_changed = Some((self.head.clone(), other.head.clone()));
        }

        let mut refs_added = HashMap::new();
        let mut refs_removed = HashMap::new();
        let mut refs_updated = HashMap::new();

        // Check for added and updated refs in 'other'
        for (ref_name, other_hash) in &other.refs {
            match self.refs.get(ref_name) {
                Some(self_hash) => {
                    if self_hash != other_hash {
                        refs_updated.insert(ref_name.clone(), (self_hash.clone(), other_hash.clone()));
                    }
                }
                None => {
                    refs_added.insert(ref_name.clone(), other_hash.clone());
                }
            }
        }

        // Check for removed refs from 'self'
        for (ref_name, self_hash) in &self.refs {
            if !other.refs.contains_key(ref_name) {
                refs_removed.insert(ref_name.clone(), self_hash.clone());
            }
        }

        Diff {
            head_changed,
            refs_added,
            refs_removed,
            refs_updated,
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

    pub fn remove_ref(&mut self, ref_name: &str) -> GitVfsResult<String> {
        match self.refs.remove(ref_name) {
            Some(hash) => Ok(hash),
            None => Err(GitVfsError::NotFound),
        }
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
        let hash = self.data_sha256(data);
        self.create_object(&hash, data)?;
        Ok(hash)
    }

    pub fn data_sha256(&self, data_to_hash: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data_to_hash);
        let result = hasher.finalize();

        hex::encode(result)
    }
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
    RequestResponse(
        request_response::Event<
            <GitVfsProtocol as libp2p::request_response::Codec>::Request,
            <GitVfsProtocol as libp2p::request_response::Codec>::Response,
        >,
    ),
    Identify(libp2p::identify::Event),
}
impl From<KademliaEvent> for GitVfsBehaviourEvent {
    fn from(v: KademliaEvent) -> Self {
        Self::Kad(v)
    }
}
impl From<mdns::Event> for GitVfsBehaviourEvent {
    fn from(v: mdns::Event) -> Self {
        Self::Mdns(v)
    }
}
impl
    From<
        request_response::Event<
            <GitVfsProtocol as libp2p::request_response::Codec>::Request,
            <GitVfsProtocol as libp2p::request_response::Codec>::Response,
        >,
    > for GitVfsBehaviourEvent
{
    fn from(
        v: request_response::Event<
            <GitVfsProtocol as libp2p::request_response::Codec>::Request,
            <GitVfsProtocol as libp2p::request_response::Codec>::Response,
        >,
    ) -> Self {
        Self::RequestResponse(v)
    }
}
impl From<libp2p::identify::Event> for GitVfsBehaviourEvent {
    fn from(v: libp2p::identify::Event) -> Self {
        Self::Identify(v)
    }
}

#[cfg(test)]
mod tests {
    use libp2p::PeerId;
    use libp2p::identity::Keypair;

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
        codec
            .write_request(&protocol, &mut io_buffer, request_data.clone())
            .await
            .unwrap();

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
        codec
            .write_response(&protocol, &mut io_buffer, response_data.clone())
            .await
            .unwrap();

        // Reset cursor and read response
        io_buffer.set_position(0);
        let read_response = codec
            .read_response(&protocol, &mut io_buffer)
            .await
            .unwrap();

        assert_eq!(read_response, response_data);
    }

    #[test]
    fn test_behaviour_event_from_mdns_event() {
        let event = mdns::Event::Discovered(vec![(
            PeerId::random(),
            "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        )]);
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
        let _git_vfs = GitVfs::new();
        let result = _git_vfs.get_object("non_existent_hash");
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
        let _git_vfs = GitVfs::new();
        let result = _git_vfs.get_ref("refs/heads/non_existent");
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
        let _git_vfs = GitVfs::new();
        let result = _git_vfs.get_head();
        assert_eq!(result, Err(GitVfsError::NotFound));
    }

    #[test]
    fn test_create_blob() {
        let mut git_vfs = GitVfs::new();
        let data = b"blob content";
        let expected_hash = "7b24cf3d897fd680e0258c1c7c23db50a5428581ed1785c08de505c381b4c4b5";

        let hash = git_vfs.create_blob(data).unwrap();
        assert_eq!(hash, expected_hash);

        let retrieved_data = git_vfs.get_object(&hash).unwrap();
        assert_eq!(retrieved_data, data);
    }

    #[test]
    fn test_create_blob_empty_data() {
        let mut git_vfs = GitVfs::new();
        let data = b"";
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // SHA-256 hash of empty string

        let hash = git_vfs.create_blob(data).unwrap();
        assert_eq!(hash, expected_hash);

        let retrieved_data = git_vfs.get_object(&hash).unwrap();
        assert_eq!(retrieved_data, data);
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

    #[test]
    fn test_git_vfs_diff() {
        // Case 1: No differences
        let vfs1_c1 = GitVfs::new();
        let vfs2_c1 = GitVfs::new();
        let diff_c1 = vfs1_c1.diff(&vfs2_c1);
        assert_eq!(diff_c1.head_changed, None);
        assert!(diff_c1.refs_added.is_empty());
        assert!(diff_c1.refs_removed.is_empty());
        assert!(diff_c1.refs_updated.is_empty());

        // Case 2: HEAD changed (vfs1 has HEAD, vfs2 has no HEAD)
        let mut vfs1_c2a = GitVfs::new();
        let vfs2_c2a = GitVfs::new();
        vfs1_c2a.create_ref("refs/heads/main", "hash1").unwrap();
        vfs1_c2a.set_head("refs/heads/main").unwrap();
        let diff_c2a = vfs1_c2a.diff(&vfs2_c2a);
        assert_eq!(diff_c2a.head_changed, Some((Some("refs/heads/main".to_string()), None)));
        assert_eq!(diff_c2a.refs_removed.len(), 1);
        assert_eq!(diff_c2a.refs_removed["refs/heads/main"], "hash1");
        assert!(diff_c2a.refs_added.is_empty());
        assert!(diff_c2a.refs_updated.is_empty());

        // Case 2b: HEAD changed (vfs1 and vfs2 have same HEAD and refs)
        let mut vfs1_c2b = GitVfs::new();
        let mut vfs2_c2b = GitVfs::new();
        vfs1_c2b.create_ref("refs/heads/main", "hash1").unwrap();
        vfs1_c2b.set_head("refs/heads/main").unwrap();
        vfs2_c2b.create_ref("refs/heads/main", "hash1").unwrap();
        vfs2_c2b.set_head("refs/heads/main").unwrap();
        let diff_c2b = vfs1_c2b.diff(&vfs2_c2b);
        assert_eq!(diff_c2b.head_changed, None);
        assert!(diff_c2b.refs_added.is_empty());
        assert!(diff_c2b.refs_removed.is_empty());
        assert!(diff_c2b.refs_updated.is_empty());

        // Case 2c: HEAD changed (vfs1 has HEAD, vfs2 has different HEAD and added ref)
        let mut vfs1_c2c = GitVfs::new();
        let mut vfs2_c2c = GitVfs::new();
        vfs1_c2c.create_ref("refs/heads/main", "hash1").unwrap();
        vfs1_c2c.set_head("refs/heads/main").unwrap();
        vfs2_c2c.create_ref("refs/heads/feature", "hash_feature").unwrap();
        vfs2_c2c.set_head("refs/heads/feature").unwrap();
        let diff_c2c = vfs1_c2c.diff(&vfs2_c2c);
        assert_eq!(diff_c2c.head_changed, Some((Some("refs/heads/main".to_string()), Some("refs/heads/feature".to_string()))));
        assert_eq!(diff_c2c.refs_added.len(), 1);
        assert_eq!(diff_c2c.refs_added["refs/heads/feature"], "hash_feature");
        assert_eq!(diff_c2c.refs_removed.len(), 1);
        assert_eq!(diff_c2c.refs_removed["refs/heads/main"], "hash1");
        assert!(diff_c2c.refs_updated.is_empty());

        // Case 3: Added references
        let vfs1_c3 = GitVfs::new();
        let mut vfs2_c3 = GitVfs::new();
        vfs2_c3.create_ref("refs/heads/dev", "hash_dev").unwrap();
        let diff_c3 = vfs1_c3.diff(&vfs2_c3);
        assert_eq!(diff_c3.head_changed, None);
        assert_eq!(diff_c3.refs_added.len(), 1);
        assert_eq!(diff_c3.refs_added["refs/heads/dev"], "hash_dev");
        assert!(diff_c3.refs_removed.is_empty());
        assert!(diff_c3.refs_updated.is_empty());

        // Case 4: Removed references
        let mut vfs1_c4 = GitVfs::new();
        let vfs2_c4 = GitVfs::new();
        vfs1_c4.create_ref("refs/heads/temp", "hash_temp").unwrap();
        let diff_c4 = vfs1_c4.diff(&vfs2_c4);
        assert_eq!(diff_c4.head_changed, None);
        assert!(diff_c4.refs_added.is_empty());
        assert_eq!(diff_c4.refs_removed.len(), 1);
        assert_eq!(diff_c4.refs_removed["refs/heads/temp"], "hash_temp");
        assert!(diff_c4.refs_updated.is_empty());

        // Case 5: Updated references
        let mut vfs1_c5 = GitVfs::new();
        let mut vfs2_c5 = GitVfs::new();
        vfs1_c5.create_ref("refs/heads/main", "hash1").unwrap();
        vfs2_c5.create_ref("refs/heads/main", "hash_updated").unwrap();
        let diff_c5 = vfs1_c5.diff(&vfs2_c5);
        assert_eq!(diff_c5.head_changed, None);
        assert!(diff_c5.refs_added.is_empty());
        assert!(diff_c5.refs_removed.is_empty());
        assert_eq!(diff_c5.refs_updated.len(), 1);
        assert_eq!(diff_c5.refs_updated["refs/heads/main"], ("hash1".to_string(), "hash_updated".to_string()));

        // Case 6: Combined changes
        let mut vfs1_c6 = GitVfs::new();
        vfs1_c6.create_ref("refs/heads/main", "hash_a1").unwrap();
        vfs1_c6.create_ref("refs/heads/feature_a", "hash_fa1").unwrap();
        vfs1_c6.set_head("refs/heads/main").unwrap();

        let mut vfs2_c6 = GitVfs::new();
        vfs2_c6.create_ref("refs/heads/main", "hash_a2").unwrap(); // Updated
        vfs2_c6.create_ref("refs/heads/feature_b", "hash_fb1").unwrap(); // Added
        vfs2_c6.set_head("refs/heads/feature_b").unwrap(); // Head changed

        let diff_c6 = vfs1_c6.diff(&vfs2_c6);
        assert_eq!(diff_c6.head_changed, Some((Some("refs/heads/main".to_string()), Some("refs/heads/feature_b".to_string()))));
        assert_eq!(diff_c6.refs_added.len(), 1);
        assert_eq!(diff_c6.refs_added["refs/heads/feature_b"], "hash_fb1");
        assert_eq!(diff_c6.refs_removed.len(), 1);
        assert_eq!(diff_c6.refs_removed["refs/heads/feature_a"], "hash_fa1");
        assert_eq!(diff_c6.refs_updated.len(), 1);
        assert_eq!(diff_c6.refs_updated["refs/heads/main"], ("hash_a1".to_string(), "hash_a2".to_string()));
    }

    // --- Test for simulating peer synchronization ---
    #[tokio::test]
    async fn test_peer_sync() {
        let mut vfs_server = GitVfs::new();
        let mut vfs_client = GitVfs::new();

        // --- Server: Populate initial state ---
        println!("\n--- Server: Initializing ---");
        let file_content_server = b"Content from server";
        let blob_hash_server = vfs_server
            .create_blob(file_content_server)
            .expect("Server: Failed to create blob");
        let ref_name_server = "refs/heads/main";
        vfs_server
            .create_ref(ref_name_server, &blob_hash_server)
            .expect("Server: Failed to create ref");
        vfs_server
            .set_head(ref_name_server)
            .expect("Server: Failed to set HEAD");
        print_vfs_state(&vfs_server, "Server");

        // --- Client: Fetch changes from server ---
        println!("\n--- Client: Fetching from Server ---");

        // Fetch object
        let server_object_data = vfs_server
            .get_object(&blob_hash_server)
            .expect("Server: Failed to get object for client");
        vfs_client
            .create_object(&blob_hash_server, &server_object_data)
            .expect("Client: Failed to create object");
        println!("Client: Created object for hash {}", blob_hash_server);

        // Fetch ref
        let server_ref_hash = vfs_server
            .get_ref(ref_name_server)
            .expect("Server: Failed to get ref for client");
        vfs_client
            .create_ref(ref_name_server, &server_ref_hash)
            .expect("Client: Failed to create ref");
        println!(
            "Client: Created ref '{}' pointing to {}",
            ref_name_server, server_ref_hash
        );

        // Fetch HEAD
        let server_head_ref = vfs_server
            .get_head()
            .expect("Server: Failed to get HEAD for client");
        vfs_client
            .set_head(&server_head_ref)
            .expect("Client: Failed to set HEAD");
        println!("Client: Set HEAD to {}", server_head_ref);

        print_vfs_state(&vfs_client, "Client");

        // --- Server: Populate new changes ---
        println!("\n--- Server: Making new changes ---");
        let new_file_content_server = b"New content from server";
        let new_blob_hash_server = vfs_server
            .create_blob(new_file_content_server)
            .expect("Server: Failed to create new blob");
        println!(
            "Server: Created new blob with hash {}",
            new_blob_hash_server
        );
        vfs_server
            .update_ref(ref_name_server, &new_blob_hash_server)
            .expect("Server: Failed to update ref");
        println!(
            "Server: Updated ref '{}' to {}",
            ref_name_server, new_blob_hash_server
        );
        // HEAD remains on main, so it implicitly points to the new commit.
        print_vfs_state(&vfs_server, "Server");

        // --- Client: Fetch updated changes from server ---
        println!("\n--- Client: Fetching updated changes from Server ---");

        // Fetch updated object
        let server_new_object_data = vfs_server
            .get_object(&new_blob_hash_server)
            .expect("Server: Failed to get new object for client");
        vfs_client
            .create_object(&new_blob_hash_server, &server_new_object_data)
            .expect("Client: Failed to create new object");
        println!(
            "Client: Created new object for hash {}",
            new_blob_hash_server
        );

        // Fetch updated ref
        let server_updated_ref_hash = vfs_server
            .get_ref(ref_name_server)
            .expect("Server: Failed to get updated ref for client");
        vfs_client
            .update_ref(ref_name_server, &server_updated_ref_hash)
            .expect("Client: Failed to update ref");
        println!(
            "Client: Updated ref '{}' to {}",
            ref_name_server, server_updated_ref_hash
        );

        print_vfs_state(&vfs_client, "Client");

        // --- Now, reverse the roles: Client becomes server, Server becomes client ---
        println!("\n--- Reversing roles: Original Client becomes New Server ---");
        let mut vfs_server_new = GitVfs::new(); // This will be the new server
        let _vfs_client_new = GitVfs::new(); // This will be the new client

        // Populate new server state (using original client's state as source)
        println!("\n--- New Server (Original Client): Initializing ---");
        let file_content_client_orig = b"Content from original client";
        let blob_hash_client_orig = vfs_client
            .create_blob(file_content_client_orig)
            .expect("Original Client: Failed to create blob");
        println!(
            "Original Client: Created blob with hash {}",
            blob_hash_client_orig
        );
        let ref_name_client_orig = "refs/heads/feature";
        vfs_client
            .create_ref(ref_name_client_orig, &blob_hash_client_orig)
            .expect("Original Client: Failed to create ref");
        println!(
            "Original Client: Created ref '{}' pointing to {}",
            ref_name_client_orig, blob_hash_client_orig
        );
        vfs_client
            .set_head(ref_name_client_orig)
            .expect("Original Client: Failed to set HEAD");
        println!("Original Client: Set HEAD to {}", ref_name_client_orig);
        print_vfs_state(&vfs_client, "Original Client");

        // --- New Client: Fetch changes from original client ---
        println!("\n--- New Client: Fetching from Original Client ---");
        let client_orig_object_data = vfs_client
            .get_object(&blob_hash_client_orig)
            .expect("Original Client: Failed to get object for new client");
        vfs_server_new
            .create_object(&blob_hash_client_orig, &client_orig_object_data)
            .expect("New Server: Failed to create object");
        println!(
            "New Server: Created object for hash {}",
            blob_hash_client_orig
        );

        let client_orig_ref_hash = vfs_client
            .get_ref(ref_name_client_orig)
            .expect("Original Client: Failed to get ref for new client");
        vfs_server_new
            .create_ref(ref_name_client_orig, &client_orig_ref_hash)
            .expect("New Server: Failed to create ref");
        println!(
            "New Server: Created ref '{}' pointing to {}",
            ref_name_client_orig, client_orig_ref_hash
        );

        let client_orig_head_ref = vfs_client
            .get_head()
            .expect("Original Client: Failed to get HEAD for new client");
        vfs_server_new
            .set_head(&client_orig_head_ref)
            .expect("New Server: Failed to set HEAD");
        println!("New Server: Set HEAD to {}", client_orig_head_ref);

        print_vfs_state(&vfs_server_new, "New Server");

        // --- Verify new server state matches original client state ---
        assert_eq!(
            vfs_server_new.get_object(&blob_hash_client_orig).unwrap(),
            client_orig_object_data
        );
        assert_eq!(
            vfs_server_new.get_ref(ref_name_client_orig).unwrap(),
            client_orig_ref_hash
        );
        assert_eq!(vfs_server_new.get_head().unwrap(), client_orig_head_ref);
        println!(
            "\n--- Verification successful: New Server state matches Original Client state ---"
        );
    }
}

#[test]
fn test_overlay_fs_module() -> GitVfsResult<()> {
    crate::overlay_vfs::create_and_test_overlay_fs().map_err(|_e| GitVfsError::InvalidOperation)
}

#[test]
fn test_altroot_fs_module() -> GitVfsResult<()> {
    crate::altroot_vfs::create_and_test_altroot_fs().map_err(|_e| GitVfsError::InvalidOperation)
}

#[test]
fn test_memory_fs_module() -> GitVfsResult<()> {
    crate::memory_vfs::create_and_test_memory_fs().map_err(|_e| GitVfsError::InvalidOperation)
}

#[test]
fn test_embedded_fs_module() -> GitVfsResult<()> {
    crate::embedded_vfs::create_and_test_embedded_fs().map_err(|_e| GitVfsError::InvalidOperation)
}
