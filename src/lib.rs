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
}