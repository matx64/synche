use crate::{
    application::persistence::interface::PersistenceInterface,
    domain::{Directory, EntryInfo, EntryKind, Peer, entry::VersionCmp},
    proto::transport::PeerHandshakeData,
};
use std::{collections::HashMap, sync::RwLock};
use tracing::warn;
use uuid::Uuid;

pub struct EntryManager<D: PersistenceInterface> {
    db: D,
    local_id: Uuid,
    directories: RwLock<HashMap<String, Directory>>,
}

impl<D: PersistenceInterface> EntryManager<D> {
    pub fn new(
        db: D,
        local_id: Uuid,
        directories: HashMap<String, Directory>,
        filesystem_entries: HashMap<String, EntryInfo>,
    ) -> Self {
        let mut entries: HashMap<String, EntryInfo> = db
            .list_all_entries(true)
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        for (name, entry) in &mut entries {
            match filesystem_entries.get(name) {
                Some(fs_entry) if fs_entry.hash != entry.hash => {
                    *entry.vv.entry(local_id).or_insert(0) += 1;
                    db.insert_or_replace_entry(&EntryInfo {
                        name: entry.name.clone(),
                        vv: entry.vv.clone(),
                        kind: fs_entry.kind.clone(),
                        hash: fs_entry.hash.clone(),
                        is_deleted: fs_entry.is_deleted,
                    })
                    .unwrap();
                }

                None => {
                    db.remove_entry(&entry.name).unwrap();
                }

                _ => {}
            }
        }

        for (name, fs_entry) in filesystem_entries {
            if !entries.contains_key(&name) {
                db.insert_or_replace_entry(&fs_entry).unwrap();
            }
        }

        Self {
            db,
            local_id,
            directories: RwLock::new(directories),
        }
    }

    pub fn list_dirs(&self) -> HashMap<String, Directory> {
        self.directories
            .read()
            .map(|dirs| dirs.clone())
            .unwrap_or_default()
    }

    pub fn insert_entry(&self, mut entry: EntryInfo) -> EntryInfo {
        entry.vv.entry(self.local_id).or_insert(0);
        self.db.insert_or_replace_entry(&entry).unwrap();
        entry
    }

    pub fn entry_created(&self, name: &str, kind: EntryKind, hash: Option<String>) -> EntryInfo {
        let entry = EntryInfo {
            name: name.to_owned(),
            kind,
            hash,
            is_deleted: false,
            vv: HashMap::from([(self.local_id, 0)]),
        };

        self.insert_entry(entry)
    }

    pub fn entry_modified(&self, name: &str, hash: Option<String>) -> Option<EntryInfo> {
        self.get_entry(name).map(|mut entry| {
            *entry.vv.entry(self.local_id).or_insert(0) += 1;

            let updated = EntryInfo {
                name: entry.name,
                kind: entry.kind,
                vv: entry.vv,
                is_deleted: entry.is_deleted,
                hash,
            };

            self.insert_entry(updated)
        })
    }

    pub fn get_entry(&self, name: &str) -> Option<EntryInfo> {
        self.db.get_entry(name).unwrap()
    }

    pub fn get_entries_to_request(
        &self,
        peer: &Peer,
        peer_entries: HashMap<String, EntryInfo>,
    ) -> Vec<EntryInfo> {
        let mut to_request = Vec::new();

        let dirs = self.directories.read().unwrap();

        for (name, peer_entry) in peer_entries {
            if dirs.contains_key(&peer_entry.get_root_parent()) {
                if let Some(mut local_entry) = self.get_entry(&name) {
                    let cmp = local_entry.compare(&peer_entry);

                    if matches!(cmp, VersionCmp::Conflict) {
                        warn!("CONFLICT IN ENTRY: {name}");
                    }

                    if matches!(cmp, VersionCmp::KeepOther) {
                        to_request.push(peer_entry);
                    } else {
                        self.merge_versions_and_insert(&mut local_entry, &peer_entry, peer.id);
                    }
                } else {
                    to_request.push(peer_entry);
                }
            }
        }

        to_request
    }

    pub fn handle_metadata(&self, peer_id: Uuid, peer_entry: &EntryInfo) -> VersionCmp {
        if let Some(mut local_entry) = self.get_entry(&peer_entry.name) {
            let cmp = local_entry.compare(peer_entry);

            if matches!(cmp, VersionCmp::Conflict) {
                warn!("CONFLICT IN ENTRY: {}", local_entry.name);
            }

            if !matches!(cmp, VersionCmp::KeepOther) {
                self.merge_versions_and_insert(&mut local_entry, peer_entry, peer_id);
            }
            cmp
        } else {
            VersionCmp::KeepOther
        }
    }

    pub fn merge_versions_and_insert(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) {
        local_entry.vv.entry(peer_id).or_insert(0);
        for (pid, pv) in &peer_entry.vv {
            let local_version = local_entry.vv.entry(*pid).or_insert(0);
            *local_version = (*local_version).max(*pv);
        }
        self.db.insert_or_replace_entry(local_entry).unwrap();
    }

    pub fn remove_entry(&self, name: &str) -> Option<EntryInfo> {
        if let Some(mut removed) = self.db.remove_entry(name).unwrap() {
            *removed.vv.entry(self.local_id).or_insert(0) += 1;
            Some(removed)
        } else {
            None
        }
    }

    pub fn remove_dir(&self, deleted: &str) -> Vec<EntryInfo> {
        let mut removed_entries = Vec::new();

        let entries = self.db.list_all_entries(false).unwrap();
        for entry in entries {
            if entry.name.starts_with(deleted) {
                if let Some(mut removed) = self.db.remove_entry(&entry.name).unwrap() {
                    *removed.vv.entry(self.local_id).or_insert(0) += 1;
                    removed_entries.push(removed);
                }
            }
        }

        removed_entries
    }

    pub fn get_handshake_data(&self) -> PeerHandshakeData {
        let directories = self
            .directories
            .read()
            .map(|dirs| dirs.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let entries = self
            .db
            .list_all_entries(false)
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect::<HashMap<String, EntryInfo>>();

        PeerHandshakeData {
            directories,
            entries,
        }
    }

    pub fn _update_dirs(&self, updated: HashMap<String, Directory>) {
        if let Ok(mut dirs) = self.directories.write() {
            dirs.clear();
            *dirs = updated;
        }
    }
}
