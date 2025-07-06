use crate::models::file::SynchedFile;
use std::{collections::HashMap, net::SocketAddr, time::SystemTime};

#[derive(Debug, Clone)]
pub struct Device {
    pub addr: SocketAddr,
    pub synched_files: HashMap<String, SynchedFile>,
    pub last_seen: SystemTime,
}

impl Device {
    pub fn new(addr: SocketAddr, synched_files: Option<HashMap<String, SynchedFile>>) -> Self {
        Self {
            addr,
            synched_files: synched_files.unwrap_or_default(),
            last_seen: SystemTime::now(),
        }
    }
}
