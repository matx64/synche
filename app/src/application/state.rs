use crate::domain::{CanonicalPath, Peer, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, net::IpAddr, path::Path, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct AppState {
    pub local_id: Uuid,
    pub hostname: String,
    local_ip: RwLock<IpAddr>,

    pub cfg_path: CanonicalPath,
    pub home_path: CanonicalPath,

    peers: RwLock<HashMap<Uuid, Peer>>,
    pub sync_dirs: RwLock<HashMap<String, SyncDirectory>>,

    pub ports: Ports,
}

#[derive(Serialize, Deserialize)]
pub struct Ports {
    pub http: u16,
    pub presence: u16,
    pub transport: u16,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let cfg_path = "./.synche";
        let config_data = ConfigFileData::init(cfg_path);

        let (home_path, cfg_path) = Self::create_required_paths(cfg_path, &config_data.home_path);

        let sync_dirs = config_data
            .sync_dirs
            .iter()
            .map(|d| (d.name.clone(), d.to_owned()))
            .collect();

        let hostname = hostname::get().unwrap().to_string_lossy().to_string();

        Arc::new(Self {
            home_path,
            cfg_path,
            hostname,
            ports: config_data.ports,
            local_id: config_data.device_id,
            sync_dirs: RwLock::new(sync_dirs),
            peers: RwLock::new(HashMap::new()),
            local_ip: RwLock::new(local_ip_address::local_ip().unwrap()),
        })
    }

    fn create_required_paths(cfg_path: &str, home_path: &str) -> (CanonicalPath, CanonicalPath) {
        fs::create_dir_all(home_path).unwrap();

        (
            CanonicalPath::new(home_path).unwrap(),
            CanonicalPath::new(cfg_path).unwrap(),
        )
    }

    pub async fn local_ip(&self) -> IpAddr {
        *self.local_ip.read().await
    }
}

#[derive(Serialize, Deserialize)]
struct ConfigFileData {
    pub device_id: Uuid,
    pub home_path: String,
    pub sync_dirs: Vec<SyncDirectory>,
    pub ports: Ports,
}

impl ConfigFileData {
    pub fn init(path: &str) -> Self {
        let path = Path::new(path).join("config.json");

        if path.exists() {
            let contents = fs::read_to_string(path).unwrap();

            serde_json::from_str(&contents).unwrap()
        } else {
            let data = Self::new_default();

            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }

            fs::write(path, serde_json::to_string(&data).unwrap()).unwrap();
            data
        }
    }

    fn new_default() -> Self {
        Self {
            device_id: Uuid::new_v4(),
            home_path: "./Synche".to_string(),
            sync_dirs: vec![SyncDirectory {
                name: "Default Folder".to_string(),
            }],
            ports: Ports {
                http: 42880,
                presence: 42881,
                transport: 42882,
            },
        }
    }
}
