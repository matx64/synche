use crate::{
    domain::{CanonicalPath, ConfigDirectory},
    utils::fs::{config_file, default_home_dir},
};
use serde::{Deserialize, Serialize};
use tokio::{fs, io};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub home_path: CanonicalPath,
    pub directory: Vec<ConfigDirectory>,
}

impl Config {
    pub async fn init() -> io::Result<Self> {
        let path = config_file();

        if path.exists() {
            let contents = fs::read_to_string(path).await?;
            let cfg: Self = toml::from_str(&contents).map_err(io::Error::other)?;

            if !cfg.home_path.exists() {
                fs::create_dir_all(&cfg.home_path).await?;
            }

            Ok(cfg)
        } else {
            let cfg = Self::default();

            let contents = toml::to_string_pretty(&cfg).map_err(io::Error::other)?;

            fs::write(path, contents).await?;
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
