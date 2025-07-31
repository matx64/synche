use crate::domain::entry::VersionVector;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: String,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub is_deleted: bool,
    pub vv: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
}

impl EntryInfo {
    pub fn absent(name: String, kind: EntryKind, vv: VersionVector) -> Self {
        Self {
            name,
            kind,
            vv,
            hash: None,
            is_deleted: true,
        }
    }

    pub fn get_root_parent(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }
}
