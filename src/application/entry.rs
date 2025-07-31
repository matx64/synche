use crate::{
    application::persistence::interface::PersistenceInterface,
    domain::{Directory, EntryInfo, EntryKind, Peer, entry::VersionVectorCmp},
    proto::transport::PeerHandshakeData,
};
use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
};
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
            .list_all_entries()
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

    pub fn insert_entry(&self, entry: &EntryInfo) {
        self.db.insert_or_replace_entry(entry).unwrap();
    }

    pub fn entry_created(&self, name: &str, kind: EntryKind, hash: Option<String>) -> EntryInfo {
        let entry = EntryInfo {
            name: name.to_owned(),
            kind,
            hash,
            is_deleted: false,
            vv: HashMap::from([(self.local_id, 0)]),
        };

        self.insert_entry(&entry);
        entry
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

            self.insert_entry(&updated);
            updated
        })
    }

    pub fn get_entry(&self, name: &str) -> Option<EntryInfo> {
        self.db.get_entry(name).unwrap()
    }

    pub fn get_entries_to_send(
        &self,
        peer: &Peer,
        peer_entries: HashMap<String, EntryInfo>,
    ) -> Vec<EntryInfo> {
        let mut result = Vec::new();

        let entries = self.db.list_all_entries().unwrap();
        for mut entry in entries {
            if let Some(peer_entry) = peer_entries.get(&entry.name) {
                let cmp = self.compare_vv(peer.id, peer_entry, &mut entry);

                self.db.insert_or_replace_entry(&entry).unwrap();

                // TODO: Handle Conflict
                if matches!(cmp, VersionVectorCmp::Conflict) {
                    warn!("CONFLICT IN ENTRY: {}", entry.name);
                }

                if matches!(cmp, VersionVectorCmp::KeepSelf) {
                    result.push(entry.to_owned());
                }
            } else if peer.directories.contains_key(&entry.get_root_parent()) {
                entry.vv.insert(peer.id, 0);
                self.db.insert_or_replace_entry(&entry).unwrap();
                result.push(entry);
            }
        }

        result
    }

    pub fn handle_metadata(&self, peer_id: Uuid, peer_entry: &EntryInfo) -> VersionVectorCmp {
        if let Some(mut local_entry) = self.get_entry(&peer_entry.name) {
            let cmp = self.compare_vv(peer_id, peer_entry, &mut local_entry);
            self.db.insert_or_replace_entry(&local_entry).unwrap();
            cmp
        } else {
            VersionVectorCmp::KeepPeer
        }
    }

    pub fn compare_vv(
        &self,
        peer_id: Uuid,
        peer_entry: &EntryInfo,
        local_entry: &mut EntryInfo,
    ) -> VersionVectorCmp {
        if local_entry.hash == peer_entry.hash {
            self.merge_versions(peer_id, peer_entry, local_entry);
            return VersionVectorCmp::Equal;
        }

        let all_peers: HashSet<Uuid> = local_entry
            .vv
            .keys()
            .chain(peer_entry.vv.keys())
            .cloned()
            .collect();

        let is_local_dominant = all_peers.iter().all(|p| {
            let local_v = local_entry.vv.get(p).unwrap_or(&0);
            let peer_v = peer_entry.vv.get(p).unwrap_or(&0);
            local_v >= peer_v
        });
        let is_peer_dominant = all_peers.iter().all(|p| {
            let peer_v = peer_entry.vv.get(p).unwrap_or(&0);
            let local_v = local_entry.vv.get(p).unwrap_or(&0);
            peer_v >= local_v
        });

        if is_local_dominant && is_peer_dominant {
            self.merge_versions(peer_id, peer_entry, local_entry);
            VersionVectorCmp::Conflict
        } else if is_local_dominant {
            self.merge_versions(peer_id, peer_entry, local_entry);
            VersionVectorCmp::KeepSelf
        } else if is_peer_dominant {
            local_entry.vv = peer_entry.vv.clone();
            local_entry.vv.entry(self.local_id).or_insert(0);
            VersionVectorCmp::KeepPeer
        } else {
            self.merge_versions(peer_id, peer_entry, local_entry);
            VersionVectorCmp::Conflict
        }
    }

    fn merge_versions(&self, peer_id: Uuid, peer_entry: &EntryInfo, local_entry: &mut EntryInfo) {
        for (pid, pv) in &peer_entry.vv {
            let local_version = local_entry.vv.entry(*pid).or_insert(0);
            *local_version = (*local_version).max(*pv);
        }
        local_entry.vv.entry(peer_id).or_insert(0);
    }

    pub fn remove_entry(&self, name: &str) -> Option<EntryInfo> {
        if let Some(mut removed) = self.db.remove_entry(name).unwrap() {
            if let Some(old_local_v) = removed.vv.get(&self.local_id) {
                removed.vv.insert(self.local_id, *old_local_v + 1);
                Some(EntryInfo::absent(removed.name, removed.kind, removed.vv))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn remove_dir(&self, deleted: &str) -> Vec<EntryInfo> {
        let mut removed_entries = Vec::new();

        let entries = self.db.list_all_entries().unwrap();

        let prefix = format!("{deleted}/");
        let to_remove: Vec<EntryInfo> = entries
            .into_iter()
            .filter(|f| f.name.starts_with(&prefix))
            .collect();

        for entry in to_remove {
            if let Some(mut removed) = self.db.remove_entry(&entry.name).unwrap() {
                *removed.vv.entry(self.local_id).or_insert(0) += 1;
                removed_entries.push(EntryInfo::absent(removed.name, removed.kind, removed.vv));
            }
        }

        removed_entries
    }

    pub fn get_sync_data(&self) -> PeerHandshakeData {
        let directories = self
            .directories
            .read()
            .map(|dirs| dirs.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let entries = self
            .db
            .list_all_entries()
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect::<HashMap<String, EntryInfo>>();

        PeerHandshakeData {
            directories,
            entries,
        }
    }

    pub fn update_dirs(&self, updated: HashMap<String, Directory>) {
        if let Ok(mut dirs) = self.directories.write() {
            dirs.clear();
            *dirs = updated;
        }
    }
}
