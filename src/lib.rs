use sha2::{Digest, Sha256};
use std::collections::HashMap;

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
        let hex_hash = hex::encode(result);

        hex_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] // Marks a function as a test
    fn test_sha256_byte_slice() {
        let data: &[u8] = b"test data";
        let expected_hash = "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9";
        let actual_hash = hex::encode(Sha256::digest(data));
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_sha256_string() {
        let data = String::from("hello world");
        let expected_hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let actual_hash = hex::encode(Sha256::digest(data.as_bytes()));
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_sha256_empty_data() {
        let data: &[u8] = b"";
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let actual_hash = hex::encode(Sha256::digest(data));
        assert_eq!(actual_hash, expected_hash);
    }

    #[test]
    fn test_sha256_multiple_updates() {
        let mut hasher = Sha256::new();
        hasher.update(b"part one ");
        hasher.update(b"part two");
        let combined_hash = hex::encode(hasher.finalize());
        let single_hash = hex::encode(Sha256::digest(b"part one part two"));
        assert_eq!(combined_hash, single_hash);
    }
}
