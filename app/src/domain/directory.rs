use crate::domain::{ConfigDirectory, RelativePath};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDirectory {
    pub name: String,
}

impl SyncDirectory {
    pub fn to_config(&self) -> ConfigDirectory {
        ConfigDirectory {
            name: RelativePath::from(self.name.to_string()),
        }
    }
}
