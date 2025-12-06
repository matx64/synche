use crate::{
    domain::{
        AppPorts, CanonicalPath, Channel, Config, ConfigDirectory, Peer, RelativePath, ServerEvent,
        SyncDirectory,
    },
    utils::fs::{config_file, device_id_file},
};
use std::{collections::HashMap, net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::RwLock};
use uuid::Uuid;

const HTTP_PORT: u16 = 42880;
const PRESENCE_PORT: u16 = 42881;
const TRANSPORT_PORT: u16 = 42882;

pub struct AppState {
    ports: AppPorts,
    local_id: Uuid,
    instance_id: Uuid,
    hostname: String,
    home_path: CanonicalPath,
    local_ip: RwLock<IpAddr>,
    pub peers: RwLock<HashMap<Uuid, Peer>>,
    pub sync_dirs: RwLock<HashMap<RelativePath, SyncDirectory>>,
    pub sse_chan: Channel<ServerEvent>,
}

impl AppState {
    pub async fn new() -> Arc<Self> {
        let config = Config::init().await.unwrap();

        let local_ip = local_ip_address::local_ip().unwrap();
        let (local_id, instance_id) = Self::init_ids().await.unwrap();

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

        let ports = AppPorts {
            http: HTTP_PORT,
            presence: PRESENCE_PORT,
            transport: TRANSPORT_PORT,
        };

        Arc::new(Self {
            ports,
            hostname,
            local_id,
            instance_id,
            peers: Default::default(),
            sse_chan: Channel::new(10),
            home_path: config.home_path,
            local_ip: RwLock::new(local_ip),
            sync_dirs: RwLock::new(sync_dirs),
        })
    }

    pub fn ports(&self) -> &AppPorts {
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

    pub fn home_path(&self) -> &CanonicalPath {
        &self.home_path
    }

    pub async fn local_ip(&self) -> IpAddr {
        *self.local_ip.read().await
    }

    pub async fn init_ids() -> io::Result<(Uuid, Uuid)> {
        let file = device_id_file();

        let local_id = if !file.exists() {
            let id = Uuid::new_v4();
            fs::write(file, id.to_string()).await?;
            id
        } else {
            let id = fs::read_to_string(file).await?;
            Uuid::parse_str(&id).map_err(io::Error::other)?
        };

        Ok((local_id, Uuid::new_v4()))
    }

    pub async fn add_dir_to_config(&self, name: &RelativePath) -> io::Result<bool> {
        let mut directory: Vec<ConfigDirectory> = {
            let dirs = self.sync_dirs.read().await;

            if dirs.contains_key(name) {
                return Ok(false);
            }

            dirs.values().map(|d| d.to_config()).collect()
        };

        directory.push(ConfigDirectory {
            name: name.to_owned(),
        });

        self.write_config(&Config {
            directory,
            home_path: self.home_path.clone(),
        })
        .await
        .map(|_| true)
    }

    pub async fn remove_dir_from_config(&self, name: &RelativePath) -> io::Result<()> {
        let directory: Vec<ConfigDirectory> = {
            let dirs = self.sync_dirs.read().await;

            if !dirs.contains_key(name) {
                return Ok(());
            }

            dirs.iter()
                .filter(|(path, _)| *path != name)
                .map(|(_, d)| d.to_config())
                .collect()
        };

        self.write_config(&Config {
            directory,
            home_path: self.home_path.clone(),
        })
        .await
    }

    async fn write_config(&self, config: &Config) -> io::Result<()> {
        let contents = toml::to_string_pretty(config).map_err(io::Error::other)?;
        fs::write(config_file(), contents).await
    }
}
