use crate::domain::entry::{VersionCmp, VersionVector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: String,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub is_removed: bool,
    pub vv: VersionVector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

impl EntryInfo {
    pub fn compare(&self, other: &EntryInfo) -> VersionCmp {
        assert_eq!(self.name, other.name);

        if self.kind == other.kind && self.hash == other.hash && self.is_removed == other.is_removed
        {
            return VersionCmp::Equal;
        }

        let all_peers: HashSet<Uuid> = self.vv.keys().chain(other.vv.keys()).cloned().collect();

        let (mut lt, mut gt) = (false, false);
        for peer in &all_peers {
            let a = *self.vv.get(peer).unwrap_or(&0);
            let b = *other.vv.get(peer).unwrap_or(&0);
            if a < b {
                lt = true;
            }
            if a > b {
                gt = true;
            }
        }

        match (lt, gt) {
            (false, false) => match self.kind {
                EntryKind::File => {
                    if self.is_removed == other.is_removed && self.hash == other.hash {
                        VersionCmp::Equal
                    } else {
                        VersionCmp::Conflict
                    }
                }
                EntryKind::Directory => {
                    if self.is_removed == other.is_removed {
                        VersionCmp::Equal
                    } else {
                        VersionCmp::Conflict
                    }
                }
            },
            (false, true) => VersionCmp::KeepSelf,
            (true, false) => VersionCmp::KeepOther,
            (true, true) => VersionCmp::Conflict,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, EntryKind::File)
    }

    pub fn get_root_parent(&self) -> String {
        self.name.split("/").next().unwrap_or_default().to_owned()
    }
}
