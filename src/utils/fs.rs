use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::io;

pub fn get_relative_path(path: &Path, base: &PathBuf) -> io::Result<String> {
    let relative = path.strip_prefix(base).map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Couldn't extract relative path: {}", err),
        )
    })?;

    Ok(relative.to_string_lossy().replace('\\', "/"))
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
