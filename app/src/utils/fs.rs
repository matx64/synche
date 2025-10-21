use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};
use tokio::io;

pub fn get_os_config_dir() -> CanonicalPath {
    let path = Path::new("./.synche");

    if !path.exists()
        && let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent).unwrap();
    }

    CanonicalPath::new(path).unwrap()
}

pub fn get_os_synche_home_dir() -> CanonicalPath {
    let path = Path::new("./Synche");

    if !path.exists()
        && let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent).unwrap();
    }

    CanonicalPath::new(path).unwrap()
}

pub fn compute_hash(path: &CanonicalPath) -> io::Result<String> {
    let mut file = File::open(path)?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let hash = format!("{:x}", Sha256::digest(&content));
    Ok(hash)
}

pub fn is_ds_store<P: AsRef<Path>>(path: P) -> bool {
    matches!(path.as_ref().file_name(), Some(name) if name == ".DS_Store")
}
