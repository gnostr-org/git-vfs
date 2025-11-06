# git-vfs

A distributed Git object store implemented in Rust, leveraging `libp2p` for peer-to-peer networking.

## Features

*   **Local Git Object Storage**: Manages Git objects such as blobs, trees, and commits using a `HashMap` for in-memory storage.
*   **Reference Management**: Supports Git references (branches, tags) and the `HEAD` pointer.
*   **Peer-to-Peer Networking**: Utilizes `libp2p` for establishing peer connections and communication.
*   **Peer Discovery**: Integrates `mDNS` for local peer discovery and `Kademlia` DHT for distributed discovery.
*   **Object Transfer Protocol**: Implements a custom request-response protocol over `libp2p` for fetching Git objects between peers.
*   **State Synchronization**: Enables synchronization of Git objects, references, and HEAD between connected peers.
*   **Hashing**: Uses SHA256 for object hashing.

## Project Structure

The core logic is contained within `src/lib.rs`, defining the `GitVfs` struct for managing Git data and the `libp2p` behaviors for networking. Unit tests are included to verify the functionality of the `GitVfs` struct and the networking components.

## Usage

This library provides the foundational components for a distributed Git system. It can be used to build decentralized version control systems or to synchronize Git repositories across a network of peers.

## Dependencies

*   `libp2p`: For peer-to-peer networking.
*   `sha2`: For SHA256 hashing.
*   `hex`: For encoding/decoding hash strings.
*   `futures`: For asynchronous operations.
*   `tokio`: For the asynchronous runtime.
*   `async-trait`: For async trait implementations.

## Tests

The project includes a comprehensive suite of unit tests covering:
*   `GitVfs` struct methods (object/ref creation, retrieval, updates).
*   The `libp2p` request-response codec for object transfer.
*   Simulation of peer synchronization, demonstrating how state can be exchanged between nodes.