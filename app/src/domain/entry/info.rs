use crate::domain::{RelativePath, VersionCmp, VersionVector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: RelativePath,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub version: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

impl EntryInfo {
    pub fn compare(&self, other: &EntryInfo) -> VersionCmp {
        if self.kind == other.kind && self.hash == other.hash {
            return VersionCmp::Equal;
        }

        let all_peers: HashSet<Uuid> = self
            .version
            .keys()
            .chain(other.version.keys())
            .cloned()
            .collect();

        let (mut lt, mut gt) = (false, false);
        for peer in &all_peers {
            let a = *self.version.get(peer).unwrap_or(&0);
            let b = *other.version.get(peer).unwrap_or(&0);
            if a < b {
                lt = true;
            }
            if a > b {
                gt = true;
            }
        }

        match (lt, gt) {
            (false, true) => VersionCmp::KeepSelf,
            (true, false) => VersionCmp::KeepOther,
            _ => VersionCmp::Conflict,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, EntryKind::File)
    }

    pub fn get_root_parent(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }

    pub fn set_removed_hash(&mut self) {
        self.hash = Some(REMOVED_HASH.to_string());
    }

    pub fn is_removed(&self) -> bool {
        matches!(self.hash.as_deref(), Some(REMOVED_HASH))
    }
}

const REMOVED_HASH: &str = "00000000000000000000000000000000";
