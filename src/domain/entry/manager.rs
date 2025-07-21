use crate::{
    domain::{Directory, FileInfo, Peer},
    proto::tcp::PeerSyncData,
};
use std::{collections::HashMap, sync::RwLock};

pub struct EntryManager {
    directories: RwLock<HashMap<String, Directory>>,
    files: RwLock<HashMap<String, FileInfo>>,
}

impl EntryManager {
    pub fn new(directories: HashMap<String, Directory>, files: HashMap<String, FileInfo>) -> Self {
        Self {
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

    pub fn insert_file(&self, entry: FileInfo) {
        if let Ok(mut files) = self.files.write() {
            files.insert(entry.name.clone(), entry);
        }
    }

    pub fn get_file(&self, name: &str) -> Option<FileInfo> {
        self.files
            .read()
            .map(|files| files.get(name).cloned())
            .unwrap_or_default()
    }

    pub fn get_files_to_send(&self, peer: &Peer) -> Vec<FileInfo> {
        let mut result = Vec::new();

        if let Ok(files) = self.files.read() {
            for file in files.values() {
                if let Some(peer_file) = peer.files.get(&file.name) {
                    if peer_file.hash != file.hash && peer_file.version < file.version {
                        result.push(file.to_owned());
                    }
                } else if peer.directories.contains_key(&file.get_dir()) {
                    result.push(file.to_owned());
                }
            }
        }

        result
    }

    pub fn remove_file(&self, name: &str) -> FileInfo {
        if let Ok(mut files) = self.files.write() {
            match files.remove(name) {
                Some(removed) => FileInfo::absent(removed.name, removed.version + 1),
                None => FileInfo::absent(name.to_owned(), 0),
            }
        } else {
            FileInfo::absent(name.to_owned(), 0)
        }
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
                if let Some(removed) = files.remove(&name) {
                    removed_files.push(FileInfo::absent(name, removed.version + 1));
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
            .map(|files| files.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        PeerSyncData { directories, files }
    }
}
