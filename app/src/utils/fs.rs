use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::{
    fs::{self, File},
    io::{self, AsyncReadExt},
};

pub async fn get_os_config_dir() -> io::Result<CanonicalPath> {
    let path = Path::new("./.synche");

    if !path.exists() {
        fs::create_dir_all(path).await?;
    }

    CanonicalPath::new(path)
}

pub async fn get_os_synche_home_dir() -> io::Result<CanonicalPath> {
    let path = Path::new("./Synche");

    if !path.exists() {
        fs::create_dir_all(path).await?;
    }

    CanonicalPath::new(path)
}

pub async fn compute_hash(path: &CanonicalPath) -> io::Result<String> {
    let mut file = File::open(path).await?;

    let mut content = Vec::new();
    file.read_to_end(&mut content).await?;

    let hash = format!("{:x}", Sha256::digest(&content));
    Ok(hash)
}

pub fn is_ds_store<P: AsRef<Path>>(path: P) -> bool {
    matches!(path.as_ref().file_name(), Some(name) if name == ".DS_Store")
}
