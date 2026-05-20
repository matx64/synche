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
    logs: CanonicalPath,
}

impl SyncheDirs {
    /// Build using the real OS conventions:
    /// - **Linux**: `$XDG_DATA_HOME` / `$XDG_CONFIG_HOME` / `$XDG_STATE_HOME`,
    ///   falling back to `~/.local/share/synche`, `~/.config/synche`, and
    ///   `~/.local/state/synche`. Logs live under the state dir.
    /// - **macOS**: `~/Library/Application Support/synche` for data and
    ///   config; `~/Library/Logs/synche` for logs.
    /// - **Windows**: `%APPDATA%\synche` for data and config;
    ///   `%LOCALAPPDATA%\synche\logs` (falling back to `%APPDATA%\synche\logs`)
    ///   for logs.
    pub fn from_os() -> io::Result<Self> {
        let data = ensure_dir(compute_os_dir(OsDir::Data)?)?;
        let config = ensure_dir(compute_os_dir(OsDir::Config)?)?;
        let logs = ensure_dir(compute_os_dir(OsDir::Logs)?)?;
        Ok(Self { data, config, logs })
    }

    /// Build directories rooted under `root`, creating
    /// `root/data`, `root/config`, and `root/logs`. Used by tests to isolate
    /// per-test state from the real OS dirs.
    #[cfg(test)]
    pub fn rooted_at<P: AsRef<Path>>(root: P) -> io::Result<Self> {
        let data = ensure_dir(root.as_ref().join("data"))?;
        let config = ensure_dir(root.as_ref().join("config"))?;
        let logs = ensure_dir(root.as_ref().join("logs"))?;
        Ok(Self { data, config, logs })
    }

    #[cfg(test)]
    pub fn data(&self) -> &CanonicalPath {
        &self.data
    }

    #[cfg(test)]
    pub fn config(&self) -> &CanonicalPath {
        &self.config
    }

    #[cfg(test)]
    pub fn logs(&self) -> &CanonicalPath {
        &self.logs
    }

    /// Path of the persistent `device_id` file. The UUID inside it is
    /// generated on first run and reused on every subsequent start.
    pub fn device_id_file(&self) -> CanonicalPath {
        self.data.join("device_id")
    }

    /// Directory where the rolling log appender writes daily files.
    pub fn log_dir(&self) -> &CanonicalPath {
        &self.logs
    }

    /// Path of `config.toml` — the user-editable settings file.
    pub fn config_file(&self) -> CanonicalPath {
        self.config.join("config.toml")
    }

    /// Path of the SQLite database that stores entry metadata.
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
    Logs,
}

fn compute_os_dir(kind: OsDir) -> io::Result<PathBuf> {
    let base: PathBuf;

    #[cfg(target_os = "linux")]
    {
        use std::env;

        let (var, fallback_segments): (&str, &[&str]) = match kind {
            OsDir::Data => ("XDG_DATA_HOME", &[".local", "share"]),
            OsDir::Config => ("XDG_CONFIG_HOME", &[".config"]),
            OsDir::Logs => ("XDG_STATE_HOME", &[".local", "state"]),
        };

        base = env::var_os(var)
            .map(PathBuf::from)
            .or_else(|| {
                dirs::home_dir().map(|home| {
                    let mut p = home;
                    for seg in fallback_segments {
                        p = p.join(seg);
                    }
                    p
                })
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Could not determine OS directory")
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
        base = match kind {
            OsDir::Data | OsDir::Config => home.join("Library").join("Application Support"),
            OsDir::Logs => home.join("Library").join("Logs"),
        };
    }

    #[cfg(target_os = "windows")]
    {
        use std::env;
        base = match kind {
            OsDir::Data | OsDir::Config => env::var_os("APPDATA")
                .map(PathBuf::from)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA not set"))?,
            OsDir::Logs => env::var_os("LOCALAPPDATA")
                .or_else(|| env::var_os("APPDATA"))
                .map(PathBuf::from)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA/APPDATA not set")
                })?,
        };
    }

    let path = base.join("synche");
    #[cfg(target_os = "windows")]
    let path = if matches!(kind, OsDir::Logs) {
        path.join("logs")
    } else {
        path
    };
    Ok(path)
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
        assert!(dirs.logs().exists());
        assert_eq!(dirs.device_id_file().file_name().unwrap(), "device_id");
        assert_eq!(dirs.config_file().file_name().unwrap(), "config.toml");
        assert_eq!(dirs.data_db_file().file_name().unwrap(), "data.db");

        assert!(dirs.device_id_file().starts_with(dirs.data().as_ref()));
        assert!(dirs.config_file().starts_with(dirs.config().as_ref()));
        assert!(dirs.log_dir().starts_with(dirs.logs().as_ref()));
    }

    #[test]
    fn rooted_at_two_distinct_roots_are_isolated() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let da = SyncheDirs::rooted_at(a.path()).unwrap();
        let db = SyncheDirs::rooted_at(b.path()).unwrap();

        assert_ne!(da.config_file(), db.config_file());
        assert_ne!(da.device_id_file(), db.device_id_file());
        assert_ne!(da.log_dir(), db.log_dir());
    }

    #[test]
    fn rooted_at_log_dir_is_separate_from_data() {
        let temp = TempDir::new().unwrap();
        let dirs = SyncheDirs::rooted_at(temp.path()).unwrap();

        assert_ne!(
            dirs.log_dir().as_ref(),
            dirs.data().as_ref(),
            "logs must not live in the data dir"
        );
    }
}
