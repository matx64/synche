use crate::domain::{ConfigDirectory, RelativePath};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDirectory {
    pub name: RelativePath,
}

impl SyncDirectory {
    pub fn to_config(&self) -> ConfigDirectory {
        ConfigDirectory {
            name: self.name.clone(),
        }
    }
}
