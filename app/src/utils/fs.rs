use crate::domain::CanonicalPath;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tokio::{
    fs::{self, File},
    io::{self, AsyncReadExt},
};

static CONFIG_DIR: OnceLock<io::Result<CanonicalPath>> = OnceLock::new();

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
pub fn get_os_config_dir() -> io::Result<&'static CanonicalPath> {
    CONFIG_DIR
        .get_or_init(|| {
            let base_dir = compute_base_dir()?;

            if !base_dir.exists() {
                std::fs::create_dir_all(&base_dir)?;
            }

            CanonicalPath::new(&base_dir)
        })
        .as_ref()
        .map_err(|e| io::Error::new(e.kind(), e.to_string()))
}

fn compute_base_dir() -> io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        use std::env;
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not determine config directory",
                )
            })?;

        return Ok(config_home.join("synche"));
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine home directory",
            )
        })?;
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("synche"));
    }

    #[cfg(target_os = "windows")]
    {
        use std::env;
        let appdata = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA not set"))?;
        return Ok(appdata.join("synche"));
    }

    #[allow(unreachable_code)]
    Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
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
