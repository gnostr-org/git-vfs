use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use serde::{Serialize, Deserialize};
use chrono::Utc;
// APPROVED DEPENDENCY: Using data_encoding for all hex operations
use data_encoding::HEXUPPER; 

// ====================================================================
// ENUMS AND ERROR HANDLING
// ====================================================================

#[derive(Debug, PartialEq)]
pub enum GitVfsError {
    NotFound,
    AlreadyExists,
    InvalidOperation,
    SerializationError(String),
    IoError(String),
}

pub type GitVfsResult<T> = Result<T, GitVfsError>;

impl From<io::Error> for GitVfsError {
    fn from(err: io::Error) -> Self {
        GitVfsError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for GitVfsError {
    fn from(err: serde_json::Error) -> Self {
        GitVfsError::SerializationError(err.to_string())
    }
}

// ====================================================================
// CORE DATA STRUCTURES (SERDE DERIVED)
// ====================================================================

const HASH_BYTE_LEN: usize = 32;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)] // Added Clone for recursive logic
pub struct TreeEntry {
    pub mode: String, 
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)] // Added Clone
pub struct Commit {
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub committer: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub object: String,
    pub object_type: String,
    pub tag_name: String,
    pub tagger: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReflogEntry {
    pub old_hash: String,
    pub new_hash: String,
    pub timestamp: String,
    pub message: String,
}

#[derive(Debug)]
pub enum GitObject {
    Blob(Vec<u8>),
    Tree(Vec<TreeEntry>),
    Commit(Commit),
    Raw(Vec<u8>), 
}

// Helper struct for remote operations
pub struct Remote<'a> {
    pub name: String,
    pub vfs: &'a mut GitVfs,
}

// ====================================================================
// MAIN VFS STRUCTURE
// ====================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct GitVfs {
    pub objects: HashMap<String, Vec<u8>>,
    pub refs: HashMap<String, String>,
    pub head: Option<String>,
    pub index: HashMap<String, (String, String)>, // path -> (mode, hash) (Staging Area)
    pub reflog: HashMap<String, Vec<ReflogEntry>>,
}

impl Default for GitVfs {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// IMPLEMENTATION
// ====================================================================

impl GitVfs {
    pub fn new() -> Self {
        GitVfs {
            objects: HashMap::new(),
            refs: HashMap::new(),
            head: None,
            index: HashMap::new(),
            reflog: HashMap::new(),
        }
    }

    // --- OBJECT HASHING & MANAGEMENT ---
    /// Computes SHA-256 and encodes to uppercase hex string using data_encoding::HEXUPPER.
    pub fn data_sha256(&mut self, data_to_hash: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data_to_hash);
        let result = hasher.finalize();
        // 🚨 REPLACED: hex::encode(result) -> HEXUPPER.encode(&result)
        HEXUPPER.encode(&result)
    }

    pub fn create_object(&mut self, hash: &str, data: &[u8]) -> GitVfsResult<()> {
        if self.objects.contains_key(hash) {
            return Err(GitVfsError::AlreadyExists);
        }
        self.objects.insert(hash.to_string(), data.to_vec());
        Ok(())
    }

    fn get_raw_object(&self, hash: &str) -> GitVfsResult<Vec<u8>> {
        match self.objects.get(hash) {
            Some(data) => Ok(data.clone()),
            None => Err(GitVfsError::NotFound),
        }
    }

    // --- REFLOGGING ---
    fn log_ref_change(&mut self, ref_name: &str, old_hash: &str, new_hash: &str, message: &str) {
        let timestamp = Utc::now().to_rfc3339(); 
        
        let entry = ReflogEntry {
            old_hash: old_hash.to_string(),
            new_hash: new_hash.to_string(),
            timestamp,
            message: message.to_string(),
        };

        self.reflog.entry(ref_name.to_string())
            .or_default()
            .push(entry);
    }
    
    pub fn get_reflog(&self, ref_name: &str) -> GitVfsResult<Vec<&ReflogEntry>> {
        let log = self.reflog.get(ref_name)
            .ok_or(GitVfsError::NotFound)?;
        
        Ok(log.iter().rev().collect())
    }

    // --- REF MANAGEMENT ---
    pub fn create_ref(&mut self, ref_name: &str, hash: &str) -> GitVfsResult<()> {
        let old_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        self.refs.insert(ref_name.to_string(), hash.to_string());
        self.log_ref_change(ref_name, old_hash, hash, "initial creation");
        Ok(())
    }

    pub fn get_ref(&self, ref_name: &str) -> GitVfsResult<String> {
        match self.refs.get(ref_name) {
            Some(hash) => Ok(hash.clone()),
            None => Err(GitVfsError::NotFound),
        }
    }

    pub fn update_ref(&mut self, ref_name: &str, hash: &str) -> GitVfsResult<()> {
        let old_hash = self.refs.get(ref_name)
            .ok_or(GitVfsError::NotFound)?
            .clone();

        self.refs.insert(ref_name.to_string(), hash.to_string());
        self.log_ref_change(ref_name, &old_hash, hash, "update reference");
        Ok(())
    }

    pub fn set_head(&mut self, ref_name: &str) -> GitVfsResult<()> {
        let old_head_ref = self.head.clone().unwrap_or_default();
        
        let old_hash = if old_head_ref.is_empty() {
            "0000000000000000000000000000000000000000000000000000000000000000".to_string()
        } else {
            self.get_ref(&old_head_ref).unwrap_or_default()
        };
        
        let new_hash = self.get_ref(ref_name).map_err(|_| GitVfsError::NotFound)?;

        self.log_ref_change("HEAD", &old_hash, &new_hash, &format!("checkout: moving from {} to {}", old_head_ref, ref_name));
        self.head = Some(ref_name.to_string());
        Ok(())
    }

    pub fn get_head(&self) -> GitVfsResult<String> {
        match &self.head {
            Some(head_ref) => Ok(head_ref.clone()),
            None => Err(GitVfsError::NotFound),
        }
    }
    
    // --- STAGING AREA (INDEX) ---
    pub fn add_to_index(&mut self, path: &str, mode: &str, data: &[u8]) -> GitVfsResult<()> {
        let hash = self.create_blob(data)?;
        self.index.insert(path.to_string(), (mode.to_string(), hash));
        Ok(())
    }

    pub fn list_index_contents(&self) -> Vec<String> {
        let mut contents: Vec<String> = self.index.iter()
            .map(|(path, (mode, hash))| {
                format!("{}: mode={}, hash={}", path, mode, hash)
            })
            .collect();
        contents.sort();
        contents
    }

    // --- OBJECT CREATION (Blob, Tree, Commit, Tag) ---
    pub fn create_blob(&mut self, data: &[u8]) -> GitVfsResult<String> {
        let content_with_header = format!("blob {}\0", data.len());
        let mut data_to_hash = content_with_header.as_bytes().to_vec();
        data_to_hash.extend_from_slice(data);

        let hash = self.data_sha256(&data_to_hash);
        self.create_object(&hash, data_to_hash.as_slice())?;
        Ok(hash)
    }

    /// Converts TreeEntries into Git's raw tree format. Decodes the hex hash string
    /// back to raw bytes using data_encoding::HEXUPPER.
    fn format_tree_entries(entries: &[TreeEntry]) -> Vec<u8> {
        let mut buffer = Vec::new();
        for entry in entries {
            let header = format!("{} {}\0", entry.mode, entry.name);
            buffer.extend_from_slice(header.as_bytes());
            // 🚨 REPLACED: hex::decode(&entry.hash) -> HEXUPPER.decode(entry.hash.as_bytes())
            let hash_bytes = HEXUPPER.decode(entry.hash.as_bytes()).expect("Invalid hash string");
            buffer.extend_from_slice(&hash_bytes);
        }
        buffer
    }

    // New helper for recursive tree building
    fn build_nested_tree_recursive(&mut self, directory_entries: HashMap<String, Vec<(String, String, String, String)>>) -> GitVfsResult<String> {
        let mut tree_entries = Vec::new();
        
        for (name, entries) in directory_entries {
            // Check if any remaining path exists (i.e., if it is a directory)
            let is_dir = entries.iter().any(|(is_file_marker, _, _, _)| is_file_marker == "dir");

            if !is_dir {
                // Case 1: File entry
                let (_, mode, hash, _) = &entries[0];
                tree_entries.push(TreeEntry {
                    mode: mode.clone(),
                    name: name.clone(),
                    hash: hash.clone(),
                });
            } else {
                // Case 2: Subdirectory
                
                // Re-group entries based on the NEXT path segment
                let subdirectory_map = entries.into_iter()
                    .filter(|(_, _, _, path)| !path.is_empty())
                    .fold(HashMap::new(), |mut acc: HashMap<String, Vec<(String, String, String, String)>>, (_, mode, hash, path)| {
                        let parts: Vec<&str> = path.splitn(2, '/').collect();
                        let next_level_name = parts[0].to_string();
                        let remaining_path = parts.get(1).map(|&s| s.to_string()).unwrap_or_default();
                        
                        let is_file_marker = if remaining_path.is_empty() { String::new() } else { "dir".to_string() };
                        
                        acc.entry(next_level_name)
                            .or_default()
                            .push((is_file_marker, mode, hash, remaining_path));
                        acc
                    });

                if subdirectory_map.is_empty() { continue; } 
                    
                let subtree_hash = self.build_nested_tree_recursive(subdirectory_map)?;
                
                tree_entries.push(TreeEntry {
                    mode: "040000".to_string(), // Directory mode
                    name,
                    hash: subtree_hash,
                });
            }
        }
        
        tree_entries.sort_by(|a, b| a.name.cmp(&b.name));
        self.create_tree(&tree_entries)
    }

    // Updated write_tree to use the recursive helper
    pub fn write_tree(&mut self) -> GitVfsResult<String> {
        // 1. Group the flat index by the top-level directory/file name
        let root_entries = self.index.iter()
            .fold(HashMap::new(), |mut acc: HashMap<String, Vec<(String, String, String, String)>>, (path, (mode, hash))| {
                let parts: Vec<&str> = path.splitn(2, '/').collect();
                let top_level_name = parts[0].to_string();
                let remaining_path = parts.get(1).map(|&s| s.to_string()).unwrap_or_default();
                
                let is_file_marker = if remaining_path.is_empty() { String::new() } else { "dir".to_string() };
                
                acc.entry(top_level_name)
                    .or_default()
                    .push((is_file_marker, mode.clone(), hash.clone(), remaining_path));
                acc
            });

        // 2. Recursively build the full tree structure
        self.build_nested_tree_recursive(root_entries)
    }
    
    pub fn create_tree(&mut self, entries: &[TreeEntry]) -> GitVfsResult<String> {
        let content_data = GitVfs::format_tree_entries(entries);
        let content_with_header = format!("tree {}\0", content_data.len());
        let mut data_to_hash = content_with_header.as_bytes().to_vec();
        data_to_hash.extend_from_slice(&content_data);

        let hash = self.data_sha256(&data_to_hash);
        self.create_object(&hash, data_to_hash.as_slice())?;
        Ok(hash)
    }

    fn format_commit(commit: &Commit) -> Vec<u8> {
        let mut content = String::new();
        content.push_str(&format!("tree {}\n", commit.tree));
        for parent in &commit.parents {
            content.push_str(&format!("parent {}\n", parent));
        }
        content.push_str(&format!("author {}\n", commit.author));
        content.push_str(&format!("committer {}\n", commit.committer));
        content.push('\n');
        content.push_str(&commit.message);
        content.into_bytes()
    }

    pub fn create_commit(&mut self, commit: &Commit) -> GitVfsResult<String> {
        let content_data = GitVfs::format_commit(commit);
        let content_with_header = format!("commit {}\0", content_data.len());
        let mut data_to_hash = content_with_header.as_bytes().to_vec();
        data_to_hash.extend_from_slice(&content_data);

        let hash = self.data_sha256(&data_to_hash);
        self.create_object(&hash, data_to_hash.as_slice())?;
        Ok(hash)
    }
    
    pub fn create_lightweight_tag(&mut self, tag_name: &str, hash: &str) -> GitVfsResult<()> {
        let full_ref = format!("refs/tags/{}", tag_name);
        if self.refs.contains_key(&full_ref) {
            return Err(GitVfsError::AlreadyExists);
        }
        self.create_ref(&full_ref, hash)?;
        Ok(())
    }

    fn format_tag(tag: &Tag) -> Vec<u8> {
        let mut content = String::new();
        content.push_str(&format!("object {}\n", tag.object));
        content.push_str(&format!("type {}\n", tag.object_type));
        content.push_str(&format!("tag {}\n", tag.tag_name));
        content.push_str(&format!("tagger {}\n", tag.tagger));
        content.push('\n');
        content.push_str(&tag.message);
        content.into_bytes()
    }

    pub fn create_annotated_tag(&mut self, tag: &Tag) -> GitVfsResult<String> {
        let content_data = GitVfs::format_tag(tag);
        let content_with_header = format!("tag {}\0", content_data.len());
        let mut data_to_hash = content_with_header.as_bytes().to_vec();
        data_to_hash.extend_from_slice(&content_data);

        let tag_object_hash = self.data_sha256(&data_to_hash);
        self.create_object(&tag_object_hash, data_to_hash.as_slice())?;
        
        let full_ref = format!("refs/tags/{}", tag.tag_name);
        self.create_ref(&full_ref, &tag_object_hash)?;

        Ok(tag_object_hash)
    }

    // --- OBJECT PARSING ---
    fn extract_object_type_and_data(raw_data: &[u8]) -> GitVfsResult<(&str, &[u8])> {
        let space_index = raw_data.iter().position(|&b| b == b' ').ok_or(GitVfsError::InvalidOperation)?;
        let null_index = raw_data.iter().position(|&b| b == b'\0').ok_or(GitVfsError::InvalidOperation)?;

        if null_index <= space_index {
            return Err(GitVfsError::InvalidOperation);
        }

        let object_type = std::str::from_utf8(&raw_data[0..space_index])
            .map_err(|_| GitVfsError::InvalidOperation)?;
        
        let object_data = &raw_data[null_index + 1..];

        Ok((object_type, object_data))
    }

    pub fn read_object(&self, hash: &str) -> GitVfsResult<GitObject> {
        let raw_object = self.get_raw_object(hash)?;
        let (obj_type, data) = GitVfs::extract_object_type_and_data(&raw_object)?;

        match obj_type {
            "blob" => Ok(GitObject::Blob(data.to_vec())),
            "tree" => self.parse_tree(data).map(GitObject::Tree),
            "commit" => self.parse_commit(data).map(GitObject::Commit),
            "tag" => self.parse_tag(data).map(|_| GitObject::Raw(raw_object)), // Simplified: just return raw for tag for now
            _ => Ok(GitObject::Raw(raw_object)),
        }
    }

    /// Parses the raw Tree object bytes into TreeEntry structs. Encodes the raw hash bytes
    /// read from the tree back into a hex string using data_encoding::HEXUPPER.
    fn parse_tree(&self, data: &[u8]) -> GitVfsResult<Vec<TreeEntry>> {
        let mut entries = Vec::new();
        let mut i = 0;
        
        while i < data.len() {
            let space_index = data[i..].iter().position(|&b| b == b' ').ok_or(GitVfsError::InvalidOperation)? + i;
            let mode = std::str::from_utf8(&data[i..space_index]).map_err(|_| GitVfsError::InvalidOperation)?.to_string();
            
            let null_index = data[space_index+1..].iter().position(|&b| b == b'\0').ok_or(GitVfsError::InvalidOperation)? + space_index + 1;
            let name = std::str::from_utf8(&data[space_index+1..null_index]).map_err(|_| GitVfsError::InvalidOperation)?.to_string();

            // Hash is always 32 bytes (256 bits) for SHA-256
            let hash_bytes = &data[null_index + 1..null_index + 1 + HASH_BYTE_LEN];
            // 🚨 REPLACED: hex::encode(hash_bytes) -> HEXUPPER.encode(hash_bytes)
            let hash = HEXUPPER.encode(hash_bytes);
            
            entries.push(TreeEntry { mode, name, hash });
            i = null_index + 1 + HASH_BYTE_LEN; 
        }
        
        Ok(entries)
    }

    fn parse_commit(&self, data: &[u8]) -> GitVfsResult<Commit> {
        let commit_content = std::str::from_utf8(data).map_err(|_| GitVfsError::InvalidOperation)?;
        let mut lines = commit_content.lines();
        
        let mut tree: Option<String> = None;
        let mut parents: Vec<String> = Vec::new();
        let mut author: Option<String> = None;
        let mut committer: Option<String> = None;

        for line in lines.by_ref() {
            if line.is_empty() {
                break;
            }
            if line.starts_with("tree ") {
                tree = Some(line[5..].to_string());
            } else if line.starts_with("parent ") {
                parents.push(line[7..].to_string());
            } else if line.starts_with("author ") {
                author = Some(line[7..].to_string());
            } else if line.starts_with("committer ") {
                committer = Some(line[10..].to_string());
            }
        }
        
        let message_raw = commit_content.find("\n\n").map(|i| &commit_content[i + 2..]);
        let message = message_raw.unwrap_or("").to_string();

        Ok(Commit {
            tree: tree.ok_or(GitVfsError::InvalidOperation)?,
            parents,
            author: author.ok_or(GitVfsError::InvalidOperation)?,
            committer: committer.ok_or(GitVfsError::InvalidOperation)?,
            message,
        })
    }
    
    // Placeholder for tag parsing
    // FIX: Added underscore to suppress unused variable warning
    fn parse_tag(&self, _data: &[u8]) -> GitVfsResult<Tag> {
        Err(GitVfsError::InvalidOperation) // Not fully implemented
    }

    // --- HISTORY & DIFFING ---
    pub fn walk_history(&self, start_commit_hash: &str) -> GitVfsResult<Vec<String>> {
        let mut history = Vec::new();
        let mut current_hash = Some(start_commit_hash.to_string());

        // Using a HashSet to prevent loops in case of merge commits/bad history
        let mut visited = HashSet::new();

        while let Some(hash) = current_hash {
            if !visited.insert(hash.clone()) {
                break; // Stop if we hit a loop
            }
            history.push(hash.clone());
            
            let parsed_object = self.read_object(&hash)?;
            if let GitObject::Commit(commit) = parsed_object {
                // Follow only the first parent (linear history)
                current_hash = commit.parents.first().cloned();
            } else {
                break;
            }
        }
        Ok(history)
    }

    fn tree_to_map(&self, tree_hash: &str) -> GitVfsResult<HashMap<String, (String, String)>> {
        let parsed_object = self.read_object(tree_hash)?;
        if let GitObject::Tree(entries) = parsed_object {
            // Need to recursively traverse trees for a full flat map
            let mut flat_map = HashMap::new();
            for entry in entries {
                if entry.mode == "040000" { // Directory/Tree
                    let subtree_map = self.tree_to_map(&entry.hash)?;
                    for (sub_path, data) in subtree_map {
                        flat_map.insert(format!("{}/{}", entry.name, sub_path), data);
                    }
                } else { // File/Blob
                    flat_map.insert(entry.name, (entry.mode, entry.hash));
                }
            }
            Ok(flat_map)

        } else {
            Err(GitVfsError::InvalidOperation)
        }
    }

    pub fn three_way_diff(&self, base_tree_hash: &str, current_tree_hash: &str, target_tree_hash: &str) -> GitVfsResult<Vec<String>> {
        let base_map = self.tree_to_map(base_tree_hash)?;
        let current_map = self.tree_to_map(current_tree_hash)?;
        let target_map = self.tree_to_map(target_tree_hash)?;

        let mut changes = Vec::new();
        let mut conflicts = Vec::new();
        
        // Collect all unique file paths across all three maps
        let all_files: HashSet<_> = base_map.keys()
            .chain(current_map.keys())
            .chain(target_map.keys())
            .collect();

        for filename in all_files {
            let base_entry = base_map.get(filename);
            let current_entry = current_map.get(filename);
            let target_entry = target_map.get(filename);
            
            let base_hash = base_entry.map(|e| &e.1);
            let current_hash = current_entry.map(|e| &e.1);
            let target_hash = target_entry.map(|e| &e.1);

            let current_changed = current_hash.map_or(base_entry.is_some(), |h| base_hash.map_or(true, |bh| h != bh));
            let target_changed = target_hash.map_or(base_entry.is_some(), |h| base_hash.map_or(true, |bh| h != bh));
            let current_deleted = current_entry.is_none() && base_entry.is_some();
            let target_deleted = target_entry.is_none() && base_entry.is_some();
            let current_added = current_entry.is_some() && base_entry.is_none();
            let target_added = target_entry.is_some() && base_entry.is_none();


            if current_added && target_added {
                if current_hash != target_hash {
                    conflicts.push(format!("CONFLICT (Add/Add): {} added independently with differing content.", filename));
                } else {
                    changes.push(format!("Added (Auto-merged): {}", filename));
                }
            }
            else if (current_deleted && target_changed) || (target_deleted && current_changed) {
                conflicts.push(format!("CONFLICT (Delete/Modify): {} deleted in one branch and modified in another.", filename));
            }
            else if current_changed && target_changed && current_hash != target_hash {
                conflicts.push(format!("CONFLICT (Content): {} modified in both branches.", filename));
            }
            // Add/Delete
            else if (current_added && target_deleted) || (target_added && current_deleted) {
                // If added in one, deleted in another. This is an explicit conflict in git merge.
                conflicts.push(format!("CONFLICT (Add/Delete): {} added in one branch and deleted in another.", filename));
            }
            else if current_deleted || target_deleted {
                changes.push(format!("Deleted: {}", filename));
            }
            else if current_added || target_added {
                changes.push(format!("Added: {}", filename));
            }
            else if current_changed || target_changed {
                changes.push(format!("Modified: {}", filename));
            }
        }

        if !conflicts.is_empty() {
            // In a real merge, we'd return the conflict list. Here we just error.
            Err(GitVfsError::InvalidOperation)
        } else {
            Ok(changes)
        }
    }

    // --- MERGING ---
    pub fn find_merge_base(&self, hash1: &str, hash2: &str) -> GitVfsResult<Option<String>> {
        let history1 = self.walk_history(hash1)?;
        let history2 = self.walk_history(hash2)?;

        for h1 in history1 {
            if history2.contains(&h1) {
                return Ok(Some(h1));
            }
        }
        Ok(None)
    }

    pub fn merge_branches(&mut self, current_branch: &str, target_branch: &str, author: &str, message: &str) -> GitVfsResult<String> {
        let current_hash = self.get_ref(current_branch)?;
        let target_hash = self.get_ref(target_branch)?;

        if current_hash == target_hash {
            return Ok(format!("Already up to date: {}", current_hash));
        }

        let merge_base_hash = self.find_merge_base(&current_hash, &target_hash)?;
        let base_hash = merge_base_hash.ok_or(GitVfsError::InvalidOperation)?;

        if base_hash == target_hash {
            return Ok(format!("Already merged. HEAD: {}", current_hash));
        }

        if base_hash == current_hash {
            // Fast-forward merge
            let parsed_object = self.read_object(&target_hash)?;
            let target_tree_hash = match parsed_object {
                GitObject::Commit(c) => c.tree,
                _ => return Err(GitVfsError::InvalidOperation),
            };
            self.update_index_from_tree(&target_tree_hash)?; // Update index to match target
            self.update_ref(current_branch, &target_hash)?;
            self.set_head(current_branch)?;
            return Ok(format!("Fast-forward merge successful. New HEAD: {}", target_hash));
        }
        
        // Simplified Three-Way Merge: Only proceed if no conflicts are detected by three_way_diff
        let current_tree = match self.read_object(&current_hash)? { GitObject::Commit(c) => c.tree, _ => return Err(GitVfsError::InvalidOperation), };
        let target_tree = match self.read_object(&target_hash)? { GitObject::Commit(c) => c.tree, _ => return Err(GitVfsError::InvalidOperation), };
        let base_tree = match self.read_object(&base_hash)? { GitObject::Commit(c) => c.tree, _ => return Err(GitVfsError::InvalidOperation), };
        
        self.three_way_diff(&base_tree, &current_tree, &target_tree)?;
        
        // If no conflicts, assume the resulting tree is the target tree (HUGE simplification of actual merge)
        let merge_commit = Commit {
            tree: target_tree.clone(),
            parents: vec![current_hash, target_hash.clone()], 
            author: author.to_string(),
            committer: author.to_string(),
            message: message.to_string(),
        };

        let new_commit_hash = self.create_commit(&merge_commit)?;
        self.update_ref(current_branch, &new_commit_hash)?;
        self.set_head(current_branch)?;
        self.update_index_from_tree(&target_tree)?; // Update index to match new tree

        Ok(format!("Merge commit created: {}", new_commit_hash))
    }
    
    // --- CHECKOUT ---
    fn update_index_from_tree(&mut self, tree_hash: &str) -> GitVfsResult<()> {
        let tree_map = self.tree_to_map(tree_hash)?;
        
        self.index.clear();
        for (path, (mode, hash)) in tree_map {
            self.index.insert(path, (mode, hash));
        }
        Ok(())
    }

    pub fn checkout(&mut self, target: &str) -> GitVfsResult<String> {
        let commit_hash = match self.get_ref(target) {
            Ok(hash) => hash,
            Err(GitVfsError::NotFound) => {
                // Check if target is a raw commit hash
                let raw_object = self.get_raw_object(target)?;
                if !String::from_utf8_lossy(&raw_object).starts_with("commit ") {
                    return Err(GitVfsError::NotFound);
                }
                target.to_string()
            },
            Err(e) => return Err(e),
        };

        let parsed_commit = self.read_object(&commit_hash)?;
        let tree_hash = match parsed_commit {
            GitObject::Commit(commit) => commit.tree,
            _ => return Err(GitVfsError::InvalidOperation),
        };

        self.update_index_from_tree(&tree_hash)?;

        if self.refs.contains_key(target) {
            self.set_head(target)?;
            Ok(format!("Switched to branch '{}'. Index updated from commit {}", target, commit_hash))
        } else {
            self.head = Some(commit_hash.clone());
            self.log_ref_change("HEAD", &self.get_reflog("HEAD").unwrap_or_default().first().map_or("0".to_string(), |e| e.new_hash.clone()), &commit_hash, &format!("checkout: moving to {}", commit_hash));
            Ok(format!("Checked out commit {}. HEAD is DETACHED. Index updated.", commit_hash))
        }
    }
    
    // --- REMOTE OPERATIONS ---
    pub fn clone_vfs(&mut self, other: &GitVfs) {
        self.objects = other.objects.clone();
        self.refs = other.refs.clone();
        // HEAD is usually not cloned, but set to the default branch (e.g., main)
        if let Some(head_ref) = other.head.as_ref() {
             let branch_name = head_ref.split('/').last().unwrap_or(head_ref);
             let full_ref = format!("refs/heads/{}", branch_name);
             self.head = Some(full_ref.clone());

             // Create remote tracking branch
             let tracking_ref = format!("refs/remotes/{}/{}", "origin", branch_name);
             if let Ok(hash) = other.get_ref(head_ref) {
                 self.refs.insert(tracking_ref, hash.clone());
             }
        }
    }
    
    pub fn push(&self, remote: &mut Remote, local_ref: &str) -> GitVfsResult<String> {
        let local_hash = self.get_ref(local_ref)?;
        let remote_ref = local_ref.to_string(); // Assuming refs/heads/main maps to refs/heads/main on remote

        // Simplified check: only allow fast-forward push
        let remote_hash = remote.vfs.get_ref(&remote_ref).unwrap_or_default();
        if !remote_hash.is_empty() {
            let merge_base = self.find_merge_base(&remote_hash, &local_hash)?;
            if merge_base != Some(remote_hash.clone()) {
                return Err(GitVfsError::InvalidOperation); // Non-fast-forward push blocked
            }
        }

        let mut transferred_count = 0;
        for (hash, data) in self.objects.iter() {
            if remote.vfs.objects.get(hash).is_none() {
                remote.vfs.objects.insert(hash.clone(), data.clone());
                transferred_count += 1;
            }
        }
        
        // Update the ref on the remote VFS (create or update)
        remote.vfs.update_ref(&remote_ref, &local_hash)
            .or_else(|_| remote.vfs.create_ref(&remote_ref, &local_hash))?;

        Ok(format!("Pushed {} to {}/{} ({} objects transferred).",
                   local_hash, remote.name, remote_ref, transferred_count))
    }

    pub fn pull(&mut self, remote: &mut Remote, local_ref: &str) -> GitVfsResult<String> {
        let remote_ref = local_ref.to_string();
        let remote_hash = remote.vfs.get_ref(&remote_ref)?;
        let branch_name = local_ref.split('/').last().unwrap_or(local_ref);
        let tracking_ref = format!("refs/remotes/{}/{}", remote.name, branch_name);

        // 1. Fetch Objects
        let mut fetched_count = 0;
        for (hash, data) in remote.vfs.objects.iter() {
            if self.objects.get(hash).is_none() {
                self.objects.insert(hash.clone(), data.clone());
                fetched_count += 1;
            }
        }

        // 2. Update Remote Tracking Ref (fetch operation)
        self.create_ref(&tracking_ref, &remote_hash).unwrap_or_else(|_| {
            self.update_ref(&tracking_ref, &remote_hash).expect("Failed to update tracking ref");
        });
        
        // 3. Determine Merge Strategy
        let local_hash = self.get_ref(local_ref).unwrap_or_default();
        
        if local_hash.is_empty() {
            // First pull/branch creation
            self.create_ref(local_ref, &remote_hash)?;
            self.set_head(local_ref)?;
            self.update_index_from_tree(&match self.read_object(&remote_hash)? { GitObject::Commit(c) => c.tree, _ => return Err(GitVfsError::InvalidOperation), })?;
            return Ok(format!("Initial pull complete. Set {} to {}. Fetched {} objects.", local_ref, remote_hash, fetched_count));
        }
        
        let merge_base_hash = self.find_merge_base(&local_hash, &remote_hash)?;
        
        if merge_base_hash == Some(local_hash.clone()) {
            // Fast-forward merge (Local is an ancestor of Remote)
            let merge_result = self.merge_branches(local_ref, &tracking_ref, "System", "Pull fast-forward merge");
            Ok(format!("Pulled and {}. Fetched {} objects.", merge_result.unwrap_or_else(|e| format!("Failed to merge: {:?}", e)), fetched_count))
        } else if merge_base_hash == Some(remote_hash.clone()) {
            // Already up to date (Remote is an ancestor of Local)
            Ok(format!("Already up to date. Fetched {} objects.", fetched_count))
        } else {
            // Diverged history (Manual merge required)
            Ok(format!("Pulled {} objects, but **diverged history** detected. Manual merge required.", fetched_count))
        }
    }

    // --- SERIALIZATION (requires file system access) ---
    pub fn save(&self, path: &str) -> GitVfsResult<()> {
        let serialized_data = serde_json::to_string_pretty(self)?;
        fs::write(path, serialized_data.as_bytes())?;
        Ok(())
    }

    pub fn load(path: &str) -> GitVfsResult<Self> {
        let mut file = fs::File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        let vfs: GitVfs = serde_json::from_str(&contents)?;
        Ok(vfs)
    }
}

// ====================================================================
// COMPREHENSIVE TEST CASES
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Test 1: Nested Tree Structure
    #[test]
    fn test_nested_tree_creation() {
        let mut git_vfs = GitVfs::new();
        
        // 1. Add files in root and in a deep subdirectory
        git_vfs.add_to_index("README.md", "100644", b"Root file.").unwrap();
        git_vfs.add_to_index("src/main.rs", "100644", b"fn main() {}").unwrap();
        git_vfs.add_to_index("src/utils/helper.rs", "100644", b"pub fn helper() {}").unwrap();

        // 2. Write the nested tree
        let root_tree_hash = git_vfs.write_tree().unwrap();
        
        // 3. Verify the root tree structure
        let root_tree_obj = git_vfs.read_object(&root_tree_hash).unwrap();
        if let GitObject::Tree(entries) = root_tree_obj {
            assert_eq!(entries.len(), 2, "Root tree should have 2 entries (README.md, src)");
            
            let src_entry = entries.iter().find(|e| e.name == "src").unwrap();
            assert_eq!(src_entry.mode, "040000", "src entry must be a directory mode");
            let src_tree_hash = src_entry.hash.clone();
            
            // 4. Verify the src tree structure
            let src_tree_obj = git_vfs.read_object(&src_tree_hash).unwrap();
            if let GitObject::Tree(src_entries) = src_tree_obj {
                assert_eq!(src_entries.len(), 2, "src tree should have 2 entries (main.rs, utils)");
                
                let utils_entry = src_entries.iter().find(|e| e.name == "utils").unwrap();
                assert_eq!(utils_entry.mode, "040000", "utils entry must be a directory mode");
                let utils_tree_hash = utils_entry.hash.clone();

                // 5. Verify the utils tree structure
                let utils_tree_obj = git_vfs.read_object(&utils_tree_hash).unwrap();
                if let GitObject::Tree(utils_entries) = utils_tree_obj {
                    assert_eq!(utils_entries.len(), 1, "utils tree should have 1 entry (helper.rs)");
                    assert_eq!(utils_entries[0].name, "helper.rs");
                    assert_eq!(utils_entries[0].mode, "100644");
                } else {
                    panic!("Utils object is not a tree");
                }
            } else {
                panic!("Src object is not a tree");
            }
        } else {
            panic!("Root object is not a tree");
        }
    }

    // Test 2: Remote Operations and Fast-Forward (Fixed Borrowing Errors)
    #[test]
    fn test_remote_tracking_push_pull_ff() {
        // --- Setup: Two VFS instances ---
        let author = "Dev User <dev@example.com>";
        let main_ref = "refs/heads/main";

        let mut remote_repo = GitVfs::new();
        let mut local_repo = GitVfs::new();

        // 1. Initial Commit C0 on Local
        local_repo.add_to_index("README.md", "100644", b"Initial remote start.").unwrap();
        let tree_c0 = local_repo.write_tree().unwrap();
        let commit_c0 = local_repo.create_commit(&Commit {
            tree: tree_c0.clone(), parents: vec![], author: author.to_string(), 
            committer: author.to_string(), message: String::from("C0: Initial local commit"),
        }).unwrap();
        local_repo.create_ref(main_ref, &commit_c0).unwrap();
        local_repo.set_head(main_ref).unwrap();
        let c0_hash = commit_c0;
        
        // 2. Initial Push (creates the remote branch)
        // FIX: Create Remote struct inline to limit the mutable borrow of remote_repo
        let _ = local_repo.push(&mut Remote { name: "origin".to_string(), vfs: &mut remote_repo }, main_ref).unwrap();
        
        let remote_c0_hash = remote_repo.get_ref(main_ref).unwrap();
        assert_eq!(remote_c0_hash, c0_hash, "Remote must hold C0 hash after push.");

        // 3. Local Commit C1 (Fast-forward change)
        local_repo.add_to_index("README.md", "100644", b"Initial remote start. Updated C1").unwrap();
        let tree_c1 = local_repo.write_tree().unwrap();
        let commit_c1 = local_repo.create_commit(&Commit {
            tree: tree_c1, parents: vec![c0_hash.clone()], 
            author: author.to_string(), committer: author.to_string(), message: String::from("C1: Local update for push"),
        }).unwrap();
        local_repo.update_ref(main_ref, &commit_c1).unwrap();
        let c1_hash = commit_c1;

        // 4. Push C1 (Fast-forward successful)
        // FIX: Create Remote struct inline
        let _ = local_repo.push(&mut Remote { name: "origin".to_string(), vfs: &mut remote_repo }, main_ref).unwrap();
        
        let remote_c1_hash = remote_repo.get_ref(main_ref).unwrap();
        assert_eq!(remote_c1_hash, c1_hash, "Remote must hold C1 hash after FF push.");
        
        // 5. Remote Commit C2 (Simulate remote change) - Mutable calls on remote_repo are now safe
        remote_repo.checkout(main_ref).unwrap();
        remote_repo.add_to_index("remote_file.txt", "100644", b"Added remotely.").unwrap();
        let remote_tree_c2 = remote_repo.write_tree().unwrap();
        let commit_c2 = remote_repo.create_commit(&Commit {
            tree: remote_tree_c2.clone(), parents: vec![c1_hash.clone()], 
            author: author.to_string(), committer: author.to_string(), message: String::from("C2: Remote update for pull"),
        }).unwrap();
        remote_repo.update_ref(main_ref, &commit_c2).unwrap();
        let c2_hash = commit_c2;
        
        // 6. Pull C2 (Local performs a fast-forward pull)
        // FIX: Create Remote struct inline
        let pull_result = local_repo.pull(&mut Remote { name: "origin".to_string(), vfs: &mut remote_repo }, main_ref).unwrap();
        assert!(pull_result.contains("Pulled and Fast-forward merge successful"), "Pull must result in a fast-forward.");
        
        // Verify local state after fast-forward pull
        let local_c2_hash = local_repo.get_ref(main_ref).unwrap();
        assert_eq!(local_c2_hash, c2_hash, "Local must hold C2 hash after FF pull.");
        
        // Verify the remote tracking branch was created/updated
        let tracking_ref_name = "refs/remotes/origin/main";
        let tracking_hash = local_repo.get_ref(tracking_ref_name).unwrap();
        assert_eq!(tracking_hash, c2_hash, "Local remote tracking ref must hold C2 hash.");
        
        // The local index should reflect the new tree (C2's tree)
        assert!(local_repo.list_index_contents().iter().any(|s| s.contains("remote_file.txt")), "Local index should contain the file added remotely.");
    }
}

fn main() {
    println!("GitVfs Example with data_encoding::HEXUPPER is functional.");
    println!("Run tests with: cargo test -- --nocapture");
}
