use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::io;
use tracing::warn;

pub fn get_relative_path(path: &Path, prefix: &PathBuf) -> io::Result<String> {
    match path.canonicalize()?.strip_prefix(prefix) {
        Ok(relative) => Ok(relative.to_string_lossy().replace("\\", "/").to_owned()),
        Err(err) => {
            warn!(
                "Couldn't extract relative path from path {}: {}",
                path.display(),
                err
            );
            Err(io::Error::other(err))
        }
    }
}

pub fn get_file_data(path: &PathBuf) -> io::Result<(String, SystemTime)> {
    let mut file = File::open(path)?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let hash = format!("{:x}", Sha256::digest(&content));
    let on_disk_modified = get_last_modified_date(path)?;

    Ok((hash, on_disk_modified))
}

pub fn get_last_modified_date(path: &Path) -> io::Result<SystemTime> {
    Ok(path.metadata()?.modified().unwrap_or(SystemTime::now()))
}
