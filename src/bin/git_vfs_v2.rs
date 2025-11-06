// --- Required Imports ---
use std::io;
use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p::{
    kad::{store::MemoryStore, QueryId, RecordKey},
    mdns,
    request_response::{self},
    swarm::{NetworkBehaviour},
    PeerId,
};
use libp2p::kad::Behaviour as Kademlia;
use libp2p::kad::Event as KademliaEvent;
// use libp2p::request_response::RequestId; // Removed as it's likely InboundRequestId or OutboundRequestId

/// The libp2p protocol for requesting a Git object.
/// The Request is a `String` (the hash), the Response is a `Vec<u8>` (the raw object data).
#[derive(Debug, Clone, Default)]
struct GitVfsProtocol;

#[async_trait::async_trait]
impl libp2p::request_response::Codec for GitVfsProtocol {

    type Protocol = libp2p::StreamProtocol;
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
    identify: libp2p::identify::Behaviour,
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

#[tokio::main]
async fn main() {
    println!("Git VFS v2 starting...");
    // TODO: Implement actual logic here
}

#[cfg(test)]
mod tests {
    use libp2p::{identity, Multiaddr};
    use libp2p::identity::Keypair;

    use super::*;
    use futures::io::Cursor;
    use libp2p::request_response::Codec;

    #[tokio::test]
    async fn test_read_write_request() {
        let mut codec = GitVfsProtocol;
        let protocol = libp2p::StreamProtocol::new("/git-vfs/1.0.0");
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
        let protocol = libp2p::StreamProtocol::new("/git-vfs/1.0.0");
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

