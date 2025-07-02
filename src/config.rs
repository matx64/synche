use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs::read_to_string,
    sync::{Arc, RwLock},
};

pub struct Config {
    pub synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SynchedFile {
    pub name: String,
    pub last_updated_at: DateTime<Utc>,
}

pub fn init() -> Config {
    let contents = read_to_string(".cfg.json").expect("Failed to read .cfg.json");

    let files: Vec<SynchedFile> = serde_json::from_str(&contents).expect("Failed to parse JSON");

    let synched_files = files
        .into_iter()
        .map(|f| (f.name.clone(), f))
        .collect::<HashMap<_, _>>();

    Config {
        synched_files: Arc::new(RwLock::new(synched_files)),
    }
}
