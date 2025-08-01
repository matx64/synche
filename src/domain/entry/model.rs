use crate::domain::entry::{VersionCmp, VersionVector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: String,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub is_deleted: bool,
    pub vv: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

    pub fn compare(&self, other: &EntryInfo) -> VersionCmp {
        if self.is_file() && other.is_file() && self.hash == other.hash {
            return VersionCmp::Equal;
        }

        let all_peers: HashSet<Uuid> = self.vv.keys().chain(other.vv.keys()).cloned().collect();

        let is_local_dominant = all_peers.iter().all(|p| {
            let local_v = self.vv.get(p).unwrap_or(&0);
            let peer_v = other.vv.get(p).unwrap_or(&0);
            local_v >= peer_v
        });

        let is_peer_dominant = all_peers.iter().all(|p| {
            let peer_v = other.vv.get(p).unwrap_or(&0);
            let local_v = self.vv.get(p).unwrap_or(&0);
            peer_v >= local_v
        });

        if is_local_dominant && is_peer_dominant {
            if self.kind == other.kind && self.is_deleted == other.is_deleted {
                if matches!(self.kind, EntryKind::Directory) {
                    VersionCmp::Equal
                } else {
                    VersionCmp::Conflict
                }
            } else {
                VersionCmp::Conflict
            }
        } else if is_local_dominant {
            VersionCmp::KeepSelf
        } else if is_peer_dominant {
            VersionCmp::KeepOther
        } else {
            VersionCmp::Conflict
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, EntryKind::File)
    }

    pub fn get_root_parent(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }
}
