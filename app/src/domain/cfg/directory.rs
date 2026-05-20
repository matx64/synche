use crate::domain::{RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};

/// On-disk representation of a single entry in the `directory = [...]`
/// list of `config.toml`. Mirrors `SyncDirectory` but exists separately
/// so the serialized config schema stays decoupled from the in-memory
/// domain type.
#[derive(Serialize, Deserialize)]
pub struct ConfigDirectory {
    pub name: RelativePath,
}

impl ConfigDirectory {
    pub fn new(name: &str) -> Self {
        Self { name: name.into() }
    }

    /// Returns the in-memory `SyncDirectory` representation.
    pub fn to_sync(&self) -> SyncDirectory {
        SyncDirectory {
            name: self.name.clone(),
        }
    }
}
