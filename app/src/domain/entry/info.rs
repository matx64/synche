use crate::domain::{RelativePath, VersionCmp, VersionVector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// Metadata for a single synchronized filesystem entry.
///
/// `hash` is `None` for directories and the SHA-256 of the contents for
/// files. `deleted` marks a tombstone: a removed entry kept in the store
/// (with `hash` cleared to `None`) so the deletion keeps propagating to
/// peers — see `mark_removed`. `version` is the `VersionVector` that
/// drives conflict resolution; see `VersionCmp`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub name: RelativePath,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub version: VersionVector,
    #[serde(default)]
    pub deleted: bool,
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
        if self.kind == other.kind && self.hash == other.hash && self.deleted == other.deleted {
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

    /// Marks the entry as a tombstone by setting the `deleted` flag and
    /// clearing the content hash, so the deletion propagates through the
    /// same metadata channel as live updates without overloading the
    /// hash namespace.
    pub fn mark_removed(&mut self) {
        self.deleted = true;
        self.hash = None;
    }

    /// Returns `true` if the entry is a tombstone.
    pub fn is_removed(&self) -> bool {
        self.deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn file(name: &str, hash: Option<&str>) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::File,
            hash: hash.map(str::to_string),
            version: HashMap::new(),
            deleted: false,
        }
    }

    fn dir(name: &str) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::Directory,
            hash: None,
            version: HashMap::new(),
            deleted: false,
        }
    }

    #[test]
    fn mark_removed_sets_flag_and_clears_hash() {
        let mut entry = file("a.txt", Some("abc"));
        assert!(!entry.is_removed());

        entry.mark_removed();

        assert!(entry.is_removed());
        assert!(entry.deleted);
        assert_eq!(entry.hash, None);
    }

    #[test]
    fn live_dir_and_dir_tombstone_are_not_equal() {
        // Both have `hash: None` and `kind: Directory`, so the comparison
        // would short-circuit to `Equal` without the `deleted` term.
        let live = dir("d");
        let mut tombstone = dir("d");
        tombstone.mark_removed();

        assert_ne!(live.compare(&tombstone), VersionCmp::Equal);
        assert_ne!(tombstone.compare(&live), VersionCmp::Equal);
    }

    #[test]
    fn two_tombstones_of_same_kind_are_equal() {
        let mut a = file("a.txt", Some("abc"));
        let mut b = file("a.txt", Some("def"));
        a.mark_removed();
        b.mark_removed();

        assert_eq!(a.compare(&b), VersionCmp::Equal);
    }

    #[test]
    fn tombstone_round_trips_through_serde() {
        let mut entry = file("a.txt", Some("abc"));
        entry.mark_removed();

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: EntryInfo = serde_json::from_str(&json).unwrap();

        assert!(decoded.is_removed());
        assert_eq!(decoded.hash, None);
    }

    #[test]
    fn missing_deleted_field_defaults_to_false() {
        // A payload that predates the `deleted` field must deserialize as
        // a live (non-tombstone) entry.
        let json = r#"{"name":"a.txt","kind":"File","hash":"abc","version":{}}"#;
        let decoded: EntryInfo = serde_json::from_str(json).unwrap();

        assert!(!decoded.is_removed());
    }
}
