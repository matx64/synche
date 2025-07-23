use crate::domain::entry::VersionVector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DELETED_FILE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub hash: String,
    pub vv: VersionVector,
}

impl FileInfo {
    pub fn absent(name: String, _version: u32) -> Self {
        Self {
            name,
            hash: DELETED_FILE_HASH.to_string(),
            vv: HashMap::new(),
        }
    }

    pub fn get_dir(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }

    pub fn is_deleted(&self) -> bool {
        self.hash == DELETED_FILE_HASH
    }
}
