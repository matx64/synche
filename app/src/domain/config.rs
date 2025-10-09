use crate::domain::SyncDirectory;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ConfigFileData {
    pub sync_directories: Vec<SyncDirectory>,
}
