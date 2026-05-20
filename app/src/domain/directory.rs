use crate::domain::{ConfigDirectory, RelativePath};
use serde::{Deserialize, Serialize};

/// A top-level synchronized directory under the Synche home path.
///
/// Sync directories are the root scopes that peers can replicate
/// independently — entries inside them are addressed by paths relative
/// to home.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDirectory {
    pub name: RelativePath,
}

impl SyncDirectory {
    /// Returns the on-disk `ConfigDirectory` representation for `config.toml`.
    pub fn to_config(&self) -> ConfigDirectory {
        ConfigDirectory {
            name: self.name.clone(),
        }
    }
}
