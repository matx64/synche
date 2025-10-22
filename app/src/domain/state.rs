use crate::{
    domain::{CanonicalPath, Config, ConfigPorts, SyncDirectory},
    utils::fs::get_os_config_dir,
};
use std::{collections::HashMap, net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::RwLock};
use uuid::Uuid;

pub struct AppState {
    pub ports: ConfigPorts,
    pub local_id: Uuid,
    pub hostname: String,
    pub home_path: CanonicalPath,
    local_ip: RwLock<IpAddr>,
}

impl AppState {
    pub fn new(config: &Config) -> Arc<Self> {
        let hostname = hostname::get().unwrap().to_string_lossy().to_string();
        let local_ip = local_ip_address::local_ip().unwrap();

        Arc::new(Self {
            hostname,
            ports: config.ports.clone(),
            local_id: config.device_id,
            home_path: config.home_path.clone(),
            local_ip: RwLock::new(local_ip),
        })
    }

    pub async fn local_ip(&self) -> IpAddr {
        *self.local_ip.read().await
    }

    pub async fn update_config_file(
        &self,
        sync_dirs: HashMap<String, SyncDirectory>,
    ) -> io::Result<()> {
        let path = get_os_config_dir().await?.join("config.toml");
        let sync_dirs = sync_dirs.values().cloned().collect();

        let config = Config {
            sync_dirs,
            device_id: self.local_id,
            ports: self.ports.clone(),
            home_path: self.home_path.clone(),
        };

        let contents =
            toml::to_string_pretty(&config).map_err(|e| io::Error::other(e.to_string()))?;

        fs::write(path, contents).await
    }
}
