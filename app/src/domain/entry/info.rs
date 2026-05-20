use crate::domain::{RelativePath, VersionCmp, VersionVector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// Metadata for a single synchronized filesystem entry.
///
/// `hash` is `None` for directories and the SHA-256 of the contents for
/// files (with the sentinel `REMOVED_HASH` marking a tombstone — see
/// `set_removed_hash`). `version` is the `VersionVector` that drives
/// conflict resolution; see `VersionCmp`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: RelativePath,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub version: VersionVector,
}

/// Whether an `EntryInfo` describes a file or a directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

impl EntryInfo {
    /// Compares two entries to decide which side wins.
    ///
    /// Equal `kind` and `hash` short-circuits to `Equal` regardless of
    /// version vectors. Otherwise the comparison walks both vectors:
    /// strictly newer on every peer → `KeepSelf`/`KeepOther`, mixed →
    /// `Conflict`. A `Conflict` is materialized as a conflict file by
    /// the caller rather than overwriting either side.
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

    pub fn get_sync_dir(&self) -> RelativePath {
        self.name.sync_dir()
    }

    /// Marks the entry as removed by stamping a sentinel hash, so the
    /// tombstone propagates through the same metadata channel as live
    /// updates.
    pub fn set_removed_hash(&mut self) {
        self.hash = Some(REMOVED_HASH.to_string());
    }

    /// Returns `true` if the entry's hash matches the removal sentinel.
    pub fn is_removed(&self) -> bool {
        matches!(self.hash.as_deref(), Some(REMOVED_HASH))
    }
}

const REMOVED_HASH: &str = "00000000000000000000000000000000";
