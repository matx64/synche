use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self},
    path::Path,
    sync::{Arc, RwLock},
};

pub struct Config {
    pub synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynchedFile {
    pub name: String,
    pub last_modified_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigSynchedFile {
    pub name: String,
}

pub fn init() -> Config {
    let contents = fs::read_to_string(".cfg.json").expect("Failed to read .cfg.json");

    let files: Vec<ConfigSynchedFile> =
        serde_json::from_str(&contents).expect("Failed to parse JSON");

    let _ = fs::create_dir_all("synche-files");

    let mut existing_files = Vec::new();
    for file in files {
        let path = Path::new("synche-files").join(&file.name);

        if let Ok(metadata) = fs::metadata(&path) {
            if metadata.is_file() {
                if let Ok(modified_time) = metadata.modified() {
                    existing_files.push(SynchedFile {
                        name: file.name.clone(),
                        last_modified_at: modified_time.into(),
                    });
                }
            }
        }
    }

    let synched_files = existing_files
        .into_iter()
        .map(|f| (f.name.clone(), f))
        .collect::<HashMap<_, _>>();

    Config {
        synched_files: Arc::new(RwLock::new(synched_files)),
    }
}
