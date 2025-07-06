use crate::models::{
    device::Device,
    file::{ConfigSynchedFile, SynchedFile},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self},
    io::Read,
    net::IpAddr,
    path::Path,
    sync::{Arc, RwLock},
};

pub struct AppState {
    pub synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    pub devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
    pub constants: AppConstants,
}

pub struct AppConstants {
    pub tcp_port: u16,
    pub broadcast_port: u16,
    pub broadcast_interval_secs: u64,
    pub device_timeout_secs: u64,
}

pub fn init() -> AppState {
    let cfg_path = ".cfg.json";
    let files_dir = "synche-files";

    let files = load_config_file(cfg_path);
    fs::create_dir_all(files_dir).unwrap();

    tracing_subscriber::fmt::init();

    AppState {
        synched_files: Arc::new(RwLock::new(build_synched_files(files, files_dir))),
        devices: Arc::new(RwLock::new(HashMap::<IpAddr, Device>::new())),
        constants: AppConstants {
            tcp_port: 8889,
            broadcast_port: 8888,
            broadcast_interval_secs: 5,
            device_timeout_secs: 15,
        },
    }
}

fn load_config_file(path: &str) -> Vec<ConfigSynchedFile> {
    let contents = fs::read_to_string(path).expect("Failed to read config file");
    serde_json::from_str(&contents).expect("Failed to parse config file")
}

fn build_synched_files(files: Vec<ConfigSynchedFile>, dir: &str) -> HashMap<String, SynchedFile> {
    let mut result = HashMap::new();

    for file in files {
        let path = Path::new(dir).join(&file.name);

        if let Ok(metadata) = fs::metadata(&path) {
            if metadata.is_file() {
                if let Ok(last_modified_at) = metadata.modified() {
                    let mut f = fs::File::open(&path).unwrap();
                    let mut content = Vec::new();
                    f.read_to_end(&mut content).unwrap();

                    result.insert(
                        file.name.clone(),
                        SynchedFile {
                            name: file.name,
                            hash: format!("{:x}", Sha256::digest(content)),
                            last_modified_at,
                        },
                    );
                }
            }
        }
    }

    result
}
