use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
};

static CONFIG_DIR: OnceLock<CanonicalPath> = OnceLock::new();

/// Returns the platform-appropriate configuration directory for Synche,
/// creating it if necessary.
///
/// The directory is chosen using sane defaults per operating system:
/// - **Linux**: `$XDG_CONFIG_HOME/synche` if `XDG_CONFIG_HOME` is set,
///   otherwise `$HOME/.config/synche`
/// - **macOS**: `$HOME/Library/Application Support/synche`
/// - **Windows**: `%APPDATA%\synche`
///
/// If the directory does not exist it will be created (including any
/// missing parent directories).
pub fn config_dir() -> &'static CanonicalPath {
    CONFIG_DIR.get_or_init(|| {
        let dir = compute_config_dir().unwrap();

        if !dir.exists() {
            std::fs::create_dir_all(&dir).unwrap();
        }

        CanonicalPath::new(&dir).unwrap()
    })
}

fn compute_config_dir() -> io::Result<PathBuf> {
    let base: PathBuf;

    #[cfg(target_os = "linux")]
    {
        use std::env;
        base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not determine config directory",
                )
            })?;
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine home directory",
            )
        })?;

        base = home.join("Library").join("Application Support");
    }

    #[cfg(target_os = "windows")]
    {
        use std::env;
        base = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA not set"))?;
    }

    Ok(base.join("synche"))
}

pub fn config_file() -> CanonicalPath {
    config_dir().join("config.toml")
}

/// Returns the default platform-appropriate home directory for Synche,
/// creating it if necessary.
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
