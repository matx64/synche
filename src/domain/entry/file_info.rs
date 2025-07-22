use crate::domain::entry::VersionVector;
use serde::{Deserialize, Serialize};

const DELETED_FILE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub hash: String,
    pub version_vector: VersionVector,
}

impl FileInfo {
    pub fn absent(name: String, version: u32) -> Self {
        Self {
            name,
            hash: DELETED_FILE_HASH.to_string(),
        }
    }

    pub fn get_dir(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }

    pub fn is_deleted(&self) -> bool {
        self.hash == DELETED_FILE_HASH
    }
}
