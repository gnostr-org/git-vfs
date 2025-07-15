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

    //Simulates creating a blob object.
    pub fn create_blob(&mut self, data: &[u8]) -> GitVfsResult<String> {
        //In a real git this would be a real hash function, but for simplicity
        //we use a string representation of the data length.
        let hash = format!("{}", data.len());
        self.create_object(&hash, data)?;
        Ok(hash)
    }

    pub fn data_sha256(&mut self, data_to_hash: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data_to_hash);
        let result = hasher.finalize();
        let hex_hash = hex::encode(result);
        println!("Original data: {:?}", String::from_utf8_lossy(data_to_hash));
        println!("SHA-256 hash: {}", hex_hash);

        hex_hash
        //// Example with another data slice
        //let another_data: &[u8] = b"This is another piece of data.";
        //let another_hash = Sha256::digest(another_data); // Using the convenience `digest` function
        //println!("Another data: {:?}", String::from_utf8_lossy(another_data));
        //println!("Another SHA-256 hash: {}", hex::encode(another_hash));

        //// Example with an empty slice
        //let empty_data: &[u8] = b"";
        //let empty_hash = Sha256::digest(empty_data);
        //println!("Empty data: {:?}", String::from_utf8_lossy(empty_data));
        //println!("Empty SHA-256 hash: {}", hex::encode(empty_hash));
    }
}
