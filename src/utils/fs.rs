use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use tokio::io;

pub fn get_relative_path(path: &CanonicalPath, base: &CanonicalPath) -> io::Result<String> {
    let relative = path
        .strip_prefix(base)
        .map_err(|err| io::Error::other(format!("Couldn't extract relative path: {err}")))?;

    Ok(relative.display().to_string().replace('\\', "/"))
}

pub fn compute_hash<P: AsRef<Path>>(path: &P) -> io::Result<String> {
    let mut file = File::open(path)?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let hash = format!("{:x}", Sha256::digest(&content));
    Ok(hash)
}

pub fn is_ds_store<P: AsRef<Path>>(path: &P) -> bool {
    matches!(path.as_ref().file_name(), Some(name) if name == ".DS_Store")
}
