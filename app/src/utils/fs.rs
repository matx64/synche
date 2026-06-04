use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::{path::Path, time::SystemTime};
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

/// Returns the lowercase hex SHA-256 of the file at `path`.
///
/// Guards against a local writer mutating the file mid-read:
/// hashing while another process keeps writing would otherwise publish a hash
/// that doesn't match the on-disk bytes, so every peer that requests it rejects
/// the transfer and the stale hash lingers until the next watcher event. We
/// snapshot a cheap `(mtime, size)` signature before and after the read; if it
/// changed, an active writer touched the file, so we recompute immediately
/// instead of waiting. After a bounded number of unstable attempts we return a
/// best-effort hash — the next watcher event recomputes once the writer settles.
pub async fn compute_hash(path: &CanonicalPath) -> io::Result<String> {
    const MAX_ATTEMPTS: usize = 3;

    let mut last_hash = None;
    for _ in 0..MAX_ATTEMPTS {
        let before = file_signature(path).await?;
        let hash = hash_file_contents(path).await?;
        let after = file_signature(path).await?;
        if before == after {
            return Ok(hash);
        }
        last_hash = Some(hash);
    }

    // The file changed across every attempt (a continuously-active local
    // writer). Return the last best-effort hash without re-reading; the next
    // watcher event recomputes once the writer settles.
    Ok(last_hash.expect("loop runs at least once with MAX_ATTEMPTS >= 1"))
}

/// Reads the file at `path` in 64 KiB chunks so memory usage stays flat
/// regardless of file size, returning the lowercase hex SHA-256.
async fn hash_file_contents(path: &CanonicalPath) -> io::Result<String> {
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

/// Cheap change-detection signature for a file: `(last-modified, byte length)`.
///
/// `modified()` is best-effort (`.ok()`): platforms that don't expose an mtime
/// fall back to size-only change detection.
async fn file_signature(path: &CanonicalPath) -> io::Result<(Option<SystemTime>, u64)> {
    let meta = tokio::fs::metadata(path).await?;
    Ok((meta.modified().ok(), meta.len()))
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

    /// SHA-256 of `b"hello world"`, the canonical known-answer vector.
    const HELLO_WORLD_SHA256: &str =
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    fn write_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> CanonicalPath {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write test file");
        CanonicalPath::new(&path).expect("canonicalize test file")
    }

    #[tokio::test]
    async fn compute_hash_returns_correct_sha256() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = write_file(temp.path(), "hello.txt", b"hello world");

        let hash = compute_hash(&path).await.expect("compute hash");

        assert_eq!(hash, HELLO_WORLD_SHA256);
    }

    #[tokio::test]
    async fn compute_hash_stable_file_is_deterministic() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = write_file(temp.path(), "data.bin", b"deterministic payload");

        let first = compute_hash(&path).await.expect("first hash");
        let second = compute_hash(&path).await.expect("second hash");

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[tokio::test]
    async fn file_signature_detects_growth() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = write_file(temp.path(), "grow.txt", b"start");

        let before = file_signature(&path).await.expect("signature before");

        // Append bytes so the size (and almost certainly the mtime) changes.
        std::fs::write(path.as_ref(), b"start and then much more content").expect("grow test file");

        let after = file_signature(&path).await.expect("signature after");

        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn compute_hash_under_concurrent_writes_returns_a_valid_hash() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = write_file(temp.path(), "busy.bin", b"initial");
        let writer_path = path.as_ref().to_path_buf();

        // A local writer keeps mutating the file while we hash it. The retry
        // loop must still return Ok with a well-formed hash rather than error
        // out or hang.
        let writer = tokio::task::spawn_blocking(move || {
            for i in 0..200u32 {
                let body = format!("chunk-{i}-").repeat(64);
                let _ = std::fs::write(&writer_path, body.as_bytes());
            }
        });

        let hash = compute_hash(&path)
            .await
            .expect("compute hash under writes");
        writer.await.expect("writer task");

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
