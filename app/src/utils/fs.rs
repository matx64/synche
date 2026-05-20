use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
};

/// Returns the default platform-appropriate home directory for Synche,
/// creating it if necessary.
///
/// **Synche Home Directory** is the location for all synchronized data.
///
/// The directory is chosen using sane defaults per operating system:
/// - On Unix-like systems this is typically: `$HOME/Synche`
/// - On Windows this is typically: `C:\Users\<User>\Synche`
///
/// If the directory does not exist it will be created (including any
/// missing parent directories).
pub fn default_home_dir() -> io::Result<CanonicalPath> {
    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine home directory",
        )
    })?;

    let dir = home.join("Synche");

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    CanonicalPath::new(&dir)
}

/// Returns the lowercase hex SHA-256 of the file at `path`, reading
/// it in 64 KiB chunks so memory usage stays flat regardless of file
/// size.
pub async fn compute_hash(path: &CanonicalPath) -> io::Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = format!("{:x}", hasher.finalize());
    Ok(hash)
}

/// Returns `true` if `path`'s final component is the macOS metadata
/// file `.DS_Store`. These files are filtered out by the watcher and
/// the entry scanner because syncing them is never useful.
pub fn is_ds_store<P: AsRef<Path>>(path: P) -> bool {
    matches!(path.as_ref().file_name(), Some(name) if name == ".DS_Store")
}

/// Returns true if any component of `path` equals `.git`.
///
/// Matches `.git/`, `repo/.git`, `a/b/.git/objects/...` etc.
/// Does NOT match `.gitignore`, `.gitattributes`, or other `.git*` names.
pub fn is_git_path(path: &str) -> bool {
    path.split('/').any(|seg| seg == ".git")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_git_path_matches_exact_component() {
        assert!(is_git_path(".git"));
        assert!(is_git_path(".git/config"));
        assert!(is_git_path("repo/.git"));
        assert!(is_git_path("repo/.git/objects/ab/cdef"));
        assert!(is_git_path("a/b/c/.git/HEAD"));
    }

    #[test]
    fn is_git_path_does_not_match_git_prefixed_names() {
        assert!(!is_git_path(".gitignore"));
        assert!(!is_git_path("repo/.gitignore"));
        assert!(!is_git_path(".gitattributes"));
        assert!(!is_git_path("repo/.github/workflows/ci.yml"));
        assert!(!is_git_path("repo/git/something"));
        assert!(!is_git_path("repo/foo.git/bar"));
    }

    #[test]
    fn is_git_path_empty() {
        assert!(!is_git_path(""));
    }
}
