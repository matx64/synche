use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDirectory {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncDirectoryConfigured {
    pub folder_name: String,
}
