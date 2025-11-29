use crate::{
    domain::{CanonicalPath, ConfigDirectory, ConfigPorts},
    utils::fs::{config_file, default_home_dir},
};
use serde::{Deserialize, Serialize};
use tokio::{fs, io};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub device_id: Uuid,
    pub ports: ConfigPorts,
    pub home_path: CanonicalPath,
    pub directory: Vec<ConfigDirectory>,
}

impl Config {
    pub async fn init() -> io::Result<Self> {
        let path = config_file();

        if path.exists() {
            let contents = fs::read_to_string(path).await?;

            toml::from_str(&contents).map_err(|e| io::Error::other(e.to_string()))
        } else {
            let data = Self::default();

            let contents =
                toml::to_string_pretty(&data).map_err(|e| io::Error::other(e.to_string()))?;

            fs::write(path, contents).await?;
            Ok(data)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_id: Uuid::new_v4(),
            home_path: default_home_dir().unwrap(),
            directory: vec![ConfigDirectory::new("Default Folder")],
            ports: ConfigPorts {
                http: 42880,
                presence: 42881,
                transport: 42882,
            },
        }
    }
}
