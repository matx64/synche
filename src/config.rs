use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::fs::read_to_string;

pub struct Config {
    pub synched_files: Vec<SynchedFile>,
}

#[derive(Debug, Deserialize)]
pub struct SynchedFile {
    pub name: String,
    pub last_updated_at: DateTime<Utc>,
}

pub fn init() -> Config {
    let contents = read_to_string(".cfg.json").expect("Failed to read .cfg.json");

    let synched_files: Vec<SynchedFile> =
        serde_json::from_str(&contents).expect("Failed to parse JSON");

    Config { synched_files }
}
