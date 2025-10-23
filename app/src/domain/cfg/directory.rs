use crate::domain::{RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ConfigDirectory {
    pub name: RelativePath,
}

impl ConfigDirectory {
    pub fn new(name: &str) -> Self {
        Self { name: name.into() }
    }

    pub fn to_sync(&self) -> SyncDirectory {
        SyncDirectory {
            name: self.name.clone(),
        }
    }
}
