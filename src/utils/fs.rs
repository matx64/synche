use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read};
use tokio::io;

pub fn compute_hash(path: &CanonicalPath) -> io::Result<String> {
    let mut file = File::open(path)?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let hash = format!("{:x}", Sha256::digest(&content));
    Ok(hash)
}

pub fn is_ds_store(path: &CanonicalPath) -> bool {
    matches!(path.file_name(), Some(name) if name == ".DS_Store")
}
