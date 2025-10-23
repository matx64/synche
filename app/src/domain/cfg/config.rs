use crate::{
    domain::{CanonicalPath, ConfigDirectory, ConfigPorts},
    utils::fs::{get_os_config_dir, get_os_synche_home_dir},
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
        let path = get_os_config_dir().await?.join("config.toml");

        if path.exists() {
            let contents = fs::read_to_string(path).await?;

            toml::from_str(&contents).map_err(|e| io::Error::other(e.to_string()))
        } else {
            let data = Self::new_default().await?;

            let contents =
                toml::to_string_pretty(&data).map_err(|e| io::Error::other(e.to_string()))?;

            fs::write(path, contents).await?;
            Ok(data)
        }
    }

    async fn new_default() -> io::Result<Self> {
        Ok(Self {
            device_id: Uuid::new_v4(),
            home_path: get_os_synche_home_dir().await?,
            directory: vec![ConfigDirectory::new("Default Folder")],
            ports: ConfigPorts {
                http: 42880,
                presence: 42881,
                transport: 42882,
            },
        })
    }
}
