use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub exists: bool,
    pub is_dir: bool,
    pub hash: String,
    pub last_modified_at: SystemTime,
}

impl Entry {
    pub fn absent(name: &str, is_dir: bool) -> Self {
        Self {
            name: name.to_owned(),
            exists: false,
            is_dir,
            hash: String::new(),
            last_modified_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigEntry {
    pub name: String,
}
