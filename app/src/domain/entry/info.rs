use crate::domain::{RelativePath, VersionCmp, VersionVector};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use uuid::Uuid;

/// Metadata for a single synchronized filesystem entry.
///
/// `hash` is `None` for directories and the SHA-256 of the contents for
/// files. `deleted` marks a tombstone: a removed entry kept in the store
/// (with `hash` cleared to `None`) so the deletion keeps propagating to
/// peers — see `mark_removed`. `version` is the `VersionVector` that
/// drives conflict resolution; see `VersionCmp`.
#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: RelativePath,
    pub kind: EntryKind,
    pub hash: Option<String>,
    pub version: VersionVector,
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

    /// Returns `true` only for entries that are valid Transfer payloads.
    pub fn is_live_file(&self) -> bool {
        self.is_file() && !self.is_removed() && self.hash.is_some()
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

impl Serialize for EntryInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct WireEntryInfo<'a> {
            name: &'a RelativePath,
            kind: &'a EntryKind,
            hash: Option<&'a str>,
            version: &'a VersionVector,
            deleted: bool,
        }

        WireEntryInfo {
            name: &self.name,
            kind: &self.kind,
            hash: if self.deleted {
                Some(LEGACY_REMOVED_HASH)
            } else {
                self.hash.as_deref()
            },
            version: &self.version,
            deleted: self.deleted,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EntryInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireEntryInfo {
            name: RelativePath,
            kind: EntryKind,
            hash: Option<String>,
            version: VersionVector,
            #[serde(default)]
            deleted: bool,
        }

        let wire = WireEntryInfo::deserialize(deserializer)?;
        let legacy_tombstone = matches!(wire.hash.as_deref(), Some(LEGACY_REMOVED_HASH));
        let deleted = wire.deleted || legacy_tombstone;

        Ok(Self {
            name: wire.name,
            kind: wire.kind,
            hash: if deleted { None } else { wire.hash },
            version: wire.version,
            deleted,
        })
    }
}

/// Legacy tombstone sentinel from before issue #42. Kept for mixed-version
/// wire compatibility and SQLite migration only; runtime/persisted tombstones
/// are represented by `deleted = true` with `hash = None`.
const LEGACY_REMOVED_HASH: &str = "00000000000000000000000000000000";

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
    fn only_live_files_are_valid_transfer_payloads() {
        let live_file = file("a.txt", Some("abc"));
        let hashless_file = file("a.txt", None);
        let live_dir = dir("d");
        let mut tombstone = file("gone.txt", Some("abc"));
        tombstone.mark_removed();

        assert!(live_file.is_live_file());
        assert!(!hashless_file.is_live_file());
        assert!(!live_dir.is_live_file());
        assert!(!tombstone.is_live_file());
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
    fn tombstone_serializes_legacy_hash_for_mixed_version_peers() {
        let mut entry = file("a.txt", Some("abc"));
        entry.mark_removed();

        let value = serde_json::to_value(&entry).unwrap();

        assert_eq!(value["deleted"], true);
        assert_eq!(value["hash"], LEGACY_REMOVED_HASH);
        assert_eq!(
            entry.hash, None,
            "serialization must not mutate runtime hash"
        );
    }

    #[test]
    fn live_entry_serializes_real_hash_without_deleted_flag_set() {
        let entry = file("a.txt", Some("abc"));

        let value = serde_json::to_value(&entry).unwrap();

        assert_eq!(value["deleted"], false);
        assert_eq!(value["hash"], "abc");
    }

    #[test]
    fn missing_deleted_field_defaults_to_false() {
        // A payload that predates the `deleted` field must deserialize as
        // a live (non-tombstone) entry.
        let json = r#"{"name":"a.txt","kind":"File","hash":"abc","version":{}}"#;
        let decoded: EntryInfo = serde_json::from_str(json).unwrap();

        assert!(!decoded.is_removed());
    }

    #[test]
    fn legacy_removed_hash_deserializes_as_tombstone() {
        let json = r#"{"name":"a.txt","kind":"File","hash":"00000000000000000000000000000000","version":{}}"#;
        let decoded: EntryInfo = serde_json::from_str(json).unwrap();

        assert!(decoded.is_removed());
        assert_eq!(decoded.hash, None);
    }
}
