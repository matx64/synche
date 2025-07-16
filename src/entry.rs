use crate::models::{
    entry::{Directory, File},
    peer::{Peer, PeerSyncData},
};
use std::{collections::HashMap, io::ErrorKind, sync::RwLock, time::SystemTime};
use tokio::io;

pub struct EntryManager {
    directories: RwLock<HashMap<String, Directory>>,
    files: RwLock<HashMap<String, File>>,
}

impl EntryManager {
    pub fn new(directories: HashMap<String, Directory>, files: HashMap<String, File>) -> Self {
        Self {
            directories: RwLock::new(directories),
            files: RwLock::new(files),
        }
    }

    pub fn dirs(&self) -> Vec<String> {
        self.directories
            .read()
            .map(|dirs| dirs.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn insert_file(&self, entry: File) {
        if let Ok(mut files) = self.files.write() {
            files.insert(entry.name.clone(), entry);
        }
    }

    pub fn get_file(&self, name: &str) -> Option<File> {
        self.files
            .read()
            .map(|files| files.get(name).cloned())
            .unwrap_or_default()
    }

    pub fn get_files_to_send(&self, peer: &Peer) -> Vec<File> {
        let mut result = Vec::new();

        if let Ok(files) = self.files.read() {
            for file in files.values() {
                if let Some(peer_file) = peer.files.get(&file.name) {
                    if peer_file.hash != file.hash
                        && peer_file.last_modified_at < file.last_modified_at
                    {
                        result.push(file.to_owned());
                    }
                }
            }
        }

        result
    }

    pub fn remove_file(&self, name: &str) {
        if let Ok(mut files) = self.files.write() {
            files.remove(name);
        }
    }

    pub fn remove_dir(&self, deleted: &str) -> Vec<File> {
        let mut removed_files = Vec::new();

        if let Ok(mut files) = self.files.write() {
            let prefix = format!("{}/", deleted);
            let to_remove: Vec<String> = files
                .keys()
                .filter(|name| name.starts_with(&prefix))
                .cloned()
                .collect();

            for name in to_remove {
                if let Some(file) = files.remove(&name) {
                    removed_files.push(File {
                        name: file.name,
                        hash: String::new(),
                        last_modified_at: SystemTime::now(),
                    });
                }
            }
        }

        removed_files
    }

    pub fn serialize(&self) -> io::Result<String> {
        let directories = match self.directories.read() {
            Ok(dirs) => dirs.values().cloned().collect::<Vec<_>>(),
            Err(err) => {
                return Err(io::Error::other(err.to_string()));
            }
        };

        let files = match self.files.read() {
            Ok(files) => files.values().cloned().collect::<Vec<_>>(),
            Err(err) => {
                return Err(io::Error::other(err.to_string()));
            }
        };

        serde_json::to_string(&PeerSyncData { directories, files })
            .map_err(|e| io::Error::other(e.to_string()))
    }

    pub fn deserialize(&self, msg: &str) -> io::Result<PeerSyncData> {
        serde_json::from_str::<PeerSyncData>(msg)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }
}
