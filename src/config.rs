use crate::{
    models::{
        device::Device,
        file::{ConfigSynchedFile, SynchedFile},
    },
    utils::fs::{get_file_data, get_last_modified_date, get_relative_path},
};
use std::path::{Path, PathBuf};
use std::{
    collections::HashMap,
    fs::{self},
    net::IpAddr,
    sync::{Arc, RwLock},
};
use tracing::info;

pub struct AppState {
    pub synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    pub devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
    pub constants: AppConstants,
}

pub struct AppConstants {
    pub files_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub tcp_port: u16,
    pub broadcast_port: u16,
    pub broadcast_interval_secs: u64,
    pub device_timeout_secs: u64,
}

pub fn init() -> AppState {
    let cfg_path = ".cfg.json";
    let files_dir = "synche-files";

    let files = load_config_file(cfg_path);
    let (files_dir, tmp_dir) = create_dirs(files_dir);

    tracing_subscriber::fmt::init();

    AppState {
        synched_files: Arc::new(RwLock::new(build_synched_files(files, &files_dir))),
        devices: Arc::new(RwLock::new(HashMap::<IpAddr, Device>::new())),
        constants: AppConstants {
            files_dir: files_dir.to_owned(),
            tmp_dir: tmp_dir.to_owned(),
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

fn create_dirs(files_dir: &str) -> (PathBuf, PathBuf) {
    let tmp_dir = std::env::temp_dir().join(files_dir);
    let files_dir = PathBuf::new().join(files_dir);

    fs::create_dir_all(&tmp_dir).unwrap();
    fs::create_dir_all(&files_dir).unwrap();

    (files_dir, tmp_dir)
}

fn build_synched_files(files: Vec<ConfigSynchedFile>, dir: &Path) -> HashMap<String, SynchedFile> {
    let mut result = HashMap::new();

    let abs_base_path = dir.canonicalize().unwrap();

    for file in files {
        let path = dir.join(&file.name);
        let relative_path = get_relative_path(&path, &abs_base_path).unwrap();

        info!("Config file relative path: {}", relative_path);

        if !path.exists() {
            result.insert(
                relative_path.clone(),
                SynchedFile::absent(&relative_path, path.is_dir()),
            );
            continue;
        }

        if path.is_dir() {
            result.insert(
                relative_path.clone(),
                SynchedFile {
                    name: relative_path,
                    exists: true,
                    is_dir: true,
                    hash: String::new(),
                    last_modified_at: get_last_modified_date(&path).unwrap(),
                },
            );
        } else if path.is_file() {
            if let Ok((hash, last_modified_at)) = get_file_data(&path) {
                result.insert(
                    relative_path.clone(),
                    SynchedFile {
                        name: relative_path,
                        exists: true,
                        is_dir: false,
                        hash,
                        last_modified_at,
                    },
                );
            }
        }
    }

    result
}
