use serde::{Deserialize, Serialize};
use std::net::IpAddr;

const DELETED_FILE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Deserialize)]
pub struct ConfiguredDirectory {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    pub hash: String,
    pub version: u32,
    pub last_modified_by: Option<IpAddr>,
}

impl File {
    pub fn absent(name: String) -> Self {
        Self {
            name,
            hash: DELETED_FILE_HASH.to_string(),
            version: 0,
            last_modified_by: None,
        }
    }

    pub fn get_dir(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }

    pub fn is_deleted(&self) -> bool {
        self.hash == DELETED_FILE_HASH
    }
}
