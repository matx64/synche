use crate::{
    domain::{Channel, Config, ConfigPorts, Peer, RelativePath, ServerEvent, SyncDirectory},
    utils::fs::{config_dir, home_dir},
};
use std::{collections::HashMap, net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::RwLock};
use uuid::Uuid;

pub struct AppState {
    ports: ConfigPorts,
    local_id: Uuid,
    instance_id: Uuid,
    hostname: String,
    local_ip: RwLock<IpAddr>,
    pub peers: RwLock<HashMap<Uuid, Peer>>,
    pub sync_dirs: RwLock<HashMap<RelativePath, SyncDirectory>>,
    pub sse_chan: Channel<ServerEvent>,
}

impl AppState {
    pub async fn new() -> Arc<Self> {
        let config = Config::init().await.unwrap();

        let local_ip = local_ip_address::local_ip().unwrap();

        let hostname = hostname::get().unwrap().to_string_lossy().to_string();
        let hostname = hostname
            .strip_suffix(".local")
            .unwrap_or(&hostname)
            .to_string();

        let sync_dirs = config
            .directory
            .iter()
            .map(|d| (d.name.clone(), d.to_sync()))
            .collect();

        Arc::new(Self {
            hostname,
            ports: config.ports.clone(),
            local_id: config.device_id,
            instance_id: Uuid::new_v4(),
            peers: RwLock::new(HashMap::new()),
            sync_dirs: RwLock::new(sync_dirs),
            local_ip: RwLock::new(local_ip),
            sse_chan: Channel::new(10),
        })
    }

    pub fn ports(&self) -> &ConfigPorts {
        &self.ports
    }

    pub fn local_id(&self) -> Uuid {
        self.local_id
    }

    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    pub fn hostname(&self) -> &String {
        &self.hostname
    }

    pub async fn local_ip(&self) -> IpAddr {
        *self.local_ip.read().await
    }

    pub async fn update_config_file(&self) -> io::Result<()> {
        let path = config_dir().join("config.toml");
        let directory = {
            self.sync_dirs
                .read()
                .await
                .values()
                .map(|d| d.to_config())
                .collect()
        };

        let config = Config {
            directory,
            device_id: self.local_id,
            ports: self.ports.clone(),
            home_path: home_dir().to_owned(),
        };

        let contents =
            toml::to_string_pretty(&config).map_err(|e| io::Error::other(e.to_string()))?;

        fs::write(path, contents).await
    }
}
