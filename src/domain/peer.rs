use crate::domain::{directory::Directory, file::FileInfo};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, time::SystemTime};

#[derive(Debug, Clone)]
pub struct Peer {
    pub addr: SocketAddr,
    pub directories: HashMap<String, Directory>,
    pub files: HashMap<String, FileInfo>,
    pub last_seen: SystemTime,
}

impl Peer {
    pub fn new(addr: SocketAddr, data: Option<PeerSyncData>) -> Self {
        let (directories, files) = data.map_or_else(
            || (HashMap::new(), HashMap::new()),
            |data| {
                let directories = data
                    .directories
                    .into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect();
                let files = data
                    .files
                    .into_iter()
                    .map(|f| (f.name.clone(), f))
                    .collect();
                (directories, files)
            },
        );

        Self {
            addr,
            directories,
            files,
            last_seen: SystemTime::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerSyncData {
    pub directories: Vec<Directory>,
    pub files: Vec<FileInfo>,
}
