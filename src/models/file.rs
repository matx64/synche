use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Deserialize)]
pub struct ConfigSynchedFile {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynchedFile {
    pub name: String,
    pub hash: String,
    pub last_modified_at: SystemTime,
}

pub struct ReceivedFile {
    pub name: String,
    pub size: u64,
    pub contents: Vec<u8>,
    pub hash: String,
    pub last_modified_at: SystemTime,
}
