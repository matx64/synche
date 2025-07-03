use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self},
    io::Read,
    path::Path,
    sync::{Arc, RwLock},
    time::SystemTime,
};

pub struct Config {
    pub synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynchedFile {
    pub name: String,
    pub last_modified_at: SystemTime,
    pub hash: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfigSynchedFile {
    pub name: String,
}

pub fn init() -> Config {
    let cfg_path = ".cfg.json";
    let files_dir = "synche-files";

    let files = load_config_file(cfg_path);
    fs::create_dir_all("synche-files").unwrap();

    Config {
        synched_files: Arc::new(RwLock::new(build_synched_files(files, files_dir))),
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
