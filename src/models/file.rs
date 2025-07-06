use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynchedFile {
    pub name: String,
    pub hash: String,
    pub last_modified_at: SystemTime,
}

#[derive(Debug, Deserialize)]
pub struct ConfigSynchedFile {
    pub name: String,
}
