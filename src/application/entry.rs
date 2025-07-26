use crate::{
    domain::{Directory, FileInfo, Peer, entry::VersionVectorCmp},
    proto::transport::PeerSyncData,
};
use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
};
use tracing::warn;
use uuid::Uuid;

pub struct EntryManager {
    local_id: Uuid,
    directories: RwLock<HashMap<String, Directory>>,
    files: RwLock<HashMap<String, FileInfo>>,
}

impl EntryManager {
    pub fn new(
        local_id: Uuid,
        directories: HashMap<String, Directory>,
        files: HashMap<String, FileInfo>,
    ) -> Self {
        Self {
            local_id,
            directories: RwLock::new(directories),
            files: RwLock::new(files),
        }
    }

    pub fn list_dirs(&self) -> Vec<String> {
        self.directories
            .read()
            .map(|dirs| dirs.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn is_dir(&self, name: &str) -> bool {
        self.directories
            .read()
            .map(|dirs| dirs.get(name).is_some())
            .unwrap_or_default()
    }

    pub fn insert_file(&self, file: FileInfo) {
        if let Ok(mut files) = self.files.write() {
            files.insert(file.name.clone(), file);
        }
    }

    pub fn file_created(&self, name: &str, hash: String) -> FileInfo {
        let file = FileInfo {
            name: name.to_owned(),
            hash,
            vv: HashMap::from([(self.local_id, 0)]),
        };

        self.insert_file(file.clone());
        file
    }

    pub fn file_modified(&self, name: &str, hash: String) -> Option<FileInfo> {
        self.files.write().ok().and_then(|mut files| {
            if let Some(file) = files.get_mut(name) {
                file.hash = hash;
                *file.vv.entry(self.local_id).or_insert(0) += 1;
                Some(file.clone())
            } else {
                None
            }
        })
    }

    pub fn get_file(&self, name: &str) -> Option<FileInfo> {
        self.files
            .read()
            .map(|files| files.get(name).cloned())
            .unwrap_or_default()
    }

    pub fn get_files_to_send(
        &self,
        peer: &Peer,
        peer_files: HashMap<String, FileInfo>,
    ) -> Vec<FileInfo> {
        let mut result = Vec::new();

        if let Ok(mut files) = self.files.write() {
            for file in files.values_mut() {
                if let Some(peer_file) = peer_files.get(&file.name) {
                    let cmp = self.compare_vv(peer.id, peer_file, file);

                    // TODO: Handle Conflict
                    if matches!(cmp, VersionVectorCmp::Conflict) {
                        warn!("CONFLICT IN FILE: {}", file.name);
                    }

                    if matches!(cmp, VersionVectorCmp::KeepSelf) {
                        result.push(file.to_owned());
                    }
                } else if peer.directories.contains_key(&file.get_dir()) {
                    file.vv.insert(peer.id, 0);
                    result.push(file.to_owned());
                }
            }
        }

        result
    }

    pub fn handle_metadata(&self, peer_id: Uuid, peer_file: &FileInfo) -> VersionVectorCmp {
        match self.files.write().unwrap().get_mut(&peer_file.name) {
            Some(local_file) => self.compare_vv(peer_id, peer_file, local_file),
            None => VersionVectorCmp::KeepPeer,
        }
    }

    pub fn compare_vv(
        &self,
        peer_id: Uuid,
        peer_file: &FileInfo,
        local_file: &mut FileInfo,
    ) -> VersionVectorCmp {
        if local_file.hash == peer_file.hash {
            self.merge_versions(peer_id, peer_file, local_file);
            return VersionVectorCmp::Equal;
        }

        let all_peers: HashSet<Uuid> = local_file
            .vv
            .keys()
            .chain(peer_file.vv.keys())
            .cloned()
            .collect();

        let is_local_dominant = all_peers.iter().all(|p| {
            let local_v = local_file.vv.get(p).unwrap_or(&0);
            let peer_v = peer_file.vv.get(p).unwrap_or(&0);
            local_v >= peer_v
        });
        let is_peer_dominant = all_peers.iter().all(|p| {
            let peer_v = peer_file.vv.get(p).unwrap_or(&0);
            let local_v = local_file.vv.get(p).unwrap_or(&0);
            peer_v >= local_v
        });

        if is_local_dominant && is_peer_dominant {
            self.merge_versions(peer_id, peer_file, local_file);
            VersionVectorCmp::Conflict
        } else if is_local_dominant {
            self.merge_versions(peer_id, peer_file, local_file);
            VersionVectorCmp::KeepSelf
        } else if is_peer_dominant {
            local_file.vv = peer_file.vv.clone();
            local_file.vv.entry(self.local_id).or_insert(0);
            VersionVectorCmp::KeepPeer
        } else {
            self.merge_versions(peer_id, peer_file, local_file);
            VersionVectorCmp::Conflict
        }
    }

    fn merge_versions(&self, peer_id: Uuid, peer_file: &FileInfo, local_file: &mut FileInfo) {
        for (pid, pv) in &peer_file.vv {
            let local_version = local_file.vv.entry(*pid).or_insert(0);
            *local_version = (*local_version).max(*pv);
        }
        local_file.vv.entry(peer_id).or_insert(0);
    }

    pub fn remove_file(&self, name: &str) -> Option<FileInfo> {
        self.files.write().ok().and_then(|mut files| {
            if let Some(mut removed) = files.remove(name) {
                if let Some(old_version) = removed.vv.get(&self.local_id) {
                    removed.vv.insert(self.local_id, *old_version + 1);
                    Some(FileInfo::absent(name.to_owned(), removed.vv))
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    pub fn remove_dir(&self, deleted: &str) -> Vec<FileInfo> {
        let mut removed_files = Vec::new();

        if let Ok(mut files) = self.files.write() {
            let prefix = format!("{deleted}/");
            let to_remove: Vec<String> = files
                .keys()
                .filter(|name| name.starts_with(&prefix))
                .cloned()
                .collect();

            for name in to_remove {
                if let Some(mut removed) = files.remove(&name) {
                    if let Some(old_version) = removed.vv.get(&self.local_id) {
                        removed.vv.insert(self.local_id, *old_version + 1);
                        removed_files.push(FileInfo::absent(name, removed.vv));
                    }
                }
            }
        }

        removed_files
    }

    pub fn get_sync_data(&self) -> PeerSyncData {
        let directories = self
            .directories
            .read()
            .map(|dirs| dirs.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let files = self
            .files
            .read()
            .map(|files| {
                files
                    .values()
                    .cloned()
                    .map(|f| (f.name.clone(), f))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        PeerSyncData { directories, files }
    }
}
