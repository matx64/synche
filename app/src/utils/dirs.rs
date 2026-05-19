use crate::domain::CanonicalPath;
#[cfg(test)]
use std::path::Path;
use std::{io, path::PathBuf};

/// Resolved on-disk locations Synche uses for persistent state.
///
/// In production these point to the platform-appropriate OS directories.
/// In tests, [`SyncheDirs::rooted_at`] places them under a single temp
/// directory so each test gets an isolated filesystem sandbox.
#[derive(Clone, Debug)]
pub struct SyncheDirs {
    data: CanonicalPath,
    config: CanonicalPath,
}

impl SyncheDirs {
    /// Build using the real OS conventions:
    /// - **Linux**: `$XDG_DATA_HOME` / `$XDG_CONFIG_HOME`, falling back to
    ///   `~/.local/share/synche` and `~/.config/synche`.
    /// - **macOS**: `~/Library/Application Support/synche` for both.
    /// - **Windows**: `%APPDATA%\synche` for both.
    pub fn from_os() -> io::Result<Self> {
        let data = ensure_dir(compute_os_dir(OsDir::Data)?)?;
        let config = ensure_dir(compute_os_dir(OsDir::Config)?)?;
        Ok(Self { data, config })
    }

    /// Build directories rooted under `root`, creating
    /// `root/data` and `root/config`. Used by tests to isolate
    /// per-test data and config from the real OS dirs.
    #[cfg(test)]
    pub fn rooted_at<P: AsRef<Path>>(root: P) -> io::Result<Self> {
        let data = ensure_dir(root.as_ref().join("data"))?;
        let config = ensure_dir(root.as_ref().join("config"))?;
        Ok(Self { data, config })
    }

    #[cfg(test)]
    pub fn data(&self) -> &CanonicalPath {
        &self.data
    }

    #[cfg(test)]
    pub fn config(&self) -> &CanonicalPath {
        &self.config
    }

    pub fn device_id_file(&self) -> CanonicalPath {
        self.data.join("device_id")
    }

    pub fn config_file(&self) -> CanonicalPath {
        self.config.join("config.toml")
    }

    pub fn data_db_file(&self) -> CanonicalPath {
        self.data.join("data.db")
    }
}

fn ensure_dir(path: PathBuf) -> io::Result<CanonicalPath> {
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }
    CanonicalPath::new(&path)
}

enum OsDir {
    Data,
    Config,
}

fn compute_os_dir(kind: OsDir) -> io::Result<PathBuf> {
    let base: PathBuf;

    #[cfg(target_os = "linux")]
    {
        use std::env;

        let is_data = matches!(kind, OsDir::Data);
        let k = if is_data {
            "XDG_DATA_HOME"
        } else {
            "XDG_CONFIG_HOME"
        };

        base = env::var_os(k)
            .map(PathBuf::from)
            .or_else(|| {
                if is_data {
                    dirs::home_dir().map(|home| home.join(".local").join("share"))
                } else {
                    dirs::home_dir().map(|home| home.join(".config"))
                }
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Could not determine OS directory")
            })?;
    }

    #[cfg(target_os = "macos")]
    {
        let _ = kind;
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
        let _ = kind;
        base = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA not set"))?;
    }

    Ok(base.join("synche"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rooted_at_creates_data_and_config_subdirs() {
        let temp = TempDir::new().unwrap();
        let dirs = SyncheDirs::rooted_at(temp.path()).unwrap();

        assert!(dirs.data().exists());
        assert!(dirs.config().exists());
        assert_eq!(dirs.device_id_file().file_name().unwrap(), "device_id");
        assert_eq!(dirs.config_file().file_name().unwrap(), "config.toml");
        assert_eq!(dirs.data_db_file().file_name().unwrap(), "data.db");

        assert!(dirs.device_id_file().starts_with(dirs.data().as_ref()));
        assert!(dirs.config_file().starts_with(dirs.config().as_ref()));
    }

    #[test]
    fn rooted_at_two_distinct_roots_are_isolated() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let da = SyncheDirs::rooted_at(a.path()).unwrap();
        let db = SyncheDirs::rooted_at(b.path()).unwrap();

        assert_ne!(da.config_file(), db.config_file());
        assert_ne!(da.device_id_file(), db.device_id_file());
    }
}
