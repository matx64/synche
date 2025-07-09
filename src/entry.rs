use crate::models::{device::Device, entry::Entry};
use std::{collections::HashMap, io::ErrorKind, sync::RwLock};
use tokio::io;
use tracing::error;

pub struct EntryService {
    entries: RwLock<HashMap<String, Entry>>,
}

impl EntryService {
    pub fn new(entries: HashMap<String, Entry>) -> Self {
        Self {
            entries: RwLock::new(entries),
        }
    }

    pub fn insert(&self, entry: Entry) {
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(entry.name.clone(), entry);
        }
    }

    pub fn get(&self, name: &str) -> Option<Entry> {
        match self.entries.read() {
            Ok(entries) => entries.get(name).cloned(),
            Err(e) => {
                error!("Read entries error: {}", e);
                None
            }
        }
    }

    pub fn to_send(&self, peer: &Device) -> Vec<Entry> {
        self.entries
            .read()
            .map(|entries| {
                entries
                    .values()
                    .filter_map(|e| {
                        peer.synched_files
                            .get(&e.name)
                            .filter(|d| d.hash != e.hash && d.last_modified_at < e.last_modified_at)
                            .cloned()
                    })
                    .collect::<_>()
            })
            .unwrap_or_default()
    }

    pub fn handle_deletion(&self, deleted: &Entry) {
        if let Ok(mut entries) = self.entries.write() {
            if deleted.is_dir {
                let start = &format!("{}/", deleted.name);

                for entry in entries.values_mut() {
                    if entry.name.starts_with(start) {
                        *entry = Entry::absent(&entry.name, entry.is_dir);
                    }
                }
            }
            entries.insert(
                deleted.name.clone(),
                Entry::absent(&deleted.name, deleted.is_dir),
            );
        }
    }

    pub fn serialize(&self) -> io::Result<String> {
        match self.entries.read() {
            Ok(entries) => {
                let vec = entries.values().collect::<Vec<_>>();
                match serde_json::to_string(&vec) {
                    Ok(json) => Ok(json),
                    Err(err) => Err(io::Error::other(err.to_string())),
                }
            }
            Err(err) => Err(io::Error::other(err.to_string())),
        }
    }

    pub fn deserialize(&self, msg: &str) -> io::Result<HashMap<String, Entry>> {
        match serde_json::from_str::<Vec<Entry>>(msg) {
            Ok(files) => Ok(files
                .into_iter()
                .map(|f| (f.name.clone(), f))
                .collect::<HashMap<String, Entry>>()),
            Err(err) => Err(io::Error::new(ErrorKind::InvalidData, err)),
        }
    }
}
