use crate::{
    domain::{CanonicalPath, SyncDirectory},
    utils::fs::{get_os_config_dir, get_os_synche_home_dir},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub device_id: Uuid,
    pub ports: ConfigPorts,
    pub home_path: CanonicalPath,
    pub sync_dirs: Vec<SyncDirectory>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ConfigPorts {
    pub http: u16,
    pub presence: u16,
    pub transport: u16,
}

impl Config {
    pub fn init() -> Self {
        let path = get_os_config_dir().join("config.json");

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
            home_path: get_os_synche_home_dir(),
            sync_dirs: vec![SyncDirectory {
                name: "Default Folder".to_string(),
            }],
            ports: ConfigPorts {
                http: 42880,
                presence: 42881,
                transport: 42882,
            },
        }
    }

    pub fn get_sync_dirs(&self) -> HashMap<String, SyncDirectory> {
        self.sync_dirs
            .iter()
            .map(|d| (d.name.clone(), d.to_owned()))
            .collect()
    }
}
