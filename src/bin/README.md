# Binaries

This directory contains various executable binaries that demonstrate and test the `git-vfs` library.

## Binaries

*   **`git2_live_test.rs`**:
    A live test simulating a 2-node Git history exchange. It covers basic commit creation and inter-node synchronization.

*   **`git_vfs_live_test-clone.rs`** and **`git_vfs_live_test.rs`**:
    These files are identical and provide a more extensive live test for continuous Git history exchange among up to four nodes. They simulate ongoing commits and synchronization, featuring graceful shutdown handling.

*   **`git_vfs_v1.rs`**:
    This binary offers a feature-rich implementation of `GitVfs`. It includes functionalities for serialization/deserialization, reflog management, staging area (index), creation of Git objects (blobs, trees, commits, annotated tags), history traversal, diffing, merging, checkout operations, and basic remote interactions (push/pull). It also contains integrated unit tests.

*   **`git_vfs_v2.rs`**:
    A straightforward binary demonstrating fundamental `GitVfs` operations. It showcases blob creation, reference management, `HEAD` manipulation, and SHA256 hashing, serving as a simple usage example.
