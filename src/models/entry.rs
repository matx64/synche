use serde::{Deserialize, Serialize};
use std::time::SystemTime;

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
    pub last_modified_at: SystemTime,
}
