use crate::{
    domain::{CanonicalPath, ConfigDirectory},
    utils::{dirs::SyncheDirs, fs::default_home_dir},
};
use serde::{Deserialize, Serialize};
use tokio::{fs, io};

/// On-disk representation of `config.toml`.
///
/// Holds the user's chosen `home_path` and the list of sync
/// directories. Edits to this file are observed by the config watcher
/// and applied live; changing `home_path` triggers the synchronizer's
/// restart loop (see `Synchronizer::run_default_with_restart`).
#[derive(Serialize, Deserialize)]
pub struct Config {
    pub home_path: CanonicalPath,
    pub directory: Vec<ConfigDirectory>,
}

impl Config {
    /// Load `config.toml` from `dirs.config_file()`, writing a default
    /// file if none exists. The resolved `home_path` directory is created
    /// if missing.
    pub async fn init(dirs: &SyncheDirs) -> io::Result<Self> {
        let path = dirs.config_file();

        if path.exists() {
            let contents = fs::read_to_string(&path).await?;
            let cfg: Self = toml::from_str(&contents).map_err(io::Error::other)?;

            if !cfg.home_path.exists() {
                fs::create_dir_all(&cfg.home_path).await?;
            }

            Ok(cfg)
        } else {
            let cfg = Self::default();

            let contents = toml::to_string_pretty(&cfg).map_err(io::Error::other)?;

            fs::write(&path, contents).await?;
            Ok(cfg)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            home_path: default_home_dir().unwrap(),
            directory: vec![ConfigDirectory::new("Default Folder")],
        }
    }
}
