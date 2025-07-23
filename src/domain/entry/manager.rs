use crate::{
    domain::{Directory, FileInfo, Peer},
    proto::transport::PeerSyncData,
};
use std::{collections::HashMap, sync::RwLock};
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

        if let Ok(files) = self.files.read() {
            for file in files.values() {
                if let Some(peer_file) = peer_files.get(&file.name) {
                    // TODO: version vector
                } else if peer.directories.contains_key(&file.get_dir()) {
                    result.push(file.to_owned());
                }
            }
        }

        result
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
