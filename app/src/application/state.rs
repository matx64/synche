use crate::domain::{CanonicalPath, Peer, RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, net::IpAddr, path::Path, sync::RwLock};
use uuid::Uuid;

pub struct AppState {
    local_id: Uuid,
    home_path: CanonicalPath,
    cfg_path: CanonicalPath,
    ports: AppPorts,
    data: RwLock<AppStateMut>,
}

#[derive(Serialize, Deserialize)]
pub struct AppPorts {
    pub http: u16,
    pub presence: u16,
    pub transport: u16,
}

struct AppStateMut {
    local_ip: IpAddr,
    sync_dirs: HashMap<RelativePath, SyncDirectory>,
    peers: HashMap<Uuid, Peer>,
}

impl AppState {
    pub fn new() -> Self {
        let cfg_path = "./.synchev2/config.json";
        let config_data = ConfigFileData::init(cfg_path);

        todo!()
    }
}

#[derive(Serialize, Deserialize)]
pub struct ConfigFileData {
    pub device_id: Uuid,
    pub home_dir: String,
    pub sync_dirs: Vec<String>,
    pub ports: AppPorts,
}

impl ConfigFileData {
    pub fn init(path: &str) -> Self {
        let path = Path::new(path);

        if path.exists() {
            let contents = fs::read_to_string(path).unwrap();

            serde_json::from_str(&contents).unwrap()
        } else {
            let data = Self::new_default();

            fs::write(path, serde_json::to_string(&data).unwrap()).unwrap();
            data
        }
    }

    fn new_default() -> Self {
        Self {
            device_id: Uuid::new_v4(),
            home_dir: "./Synche".to_string(),
            sync_dirs: vec!["Default Folder".to_string()],
            ports: AppPorts {
                http: 42880,
                presence: 42881,
                transport: 42882,
            },
        }
    }
}
