use crate::models::entry::Entry;
use std::{collections::HashMap, net::SocketAddr, time::SystemTime};

#[derive(Debug, Clone)]
pub struct Device {
    pub addr: SocketAddr,
    pub synched_files: HashMap<String, Entry>,
    pub last_seen: SystemTime,
}

impl Device {
    pub fn new(addr: SocketAddr, synched_files: Option<HashMap<String, Entry>>) -> Self {
        Self {
            addr,
            synched_files: synched_files.unwrap_or_default(),
            last_seen: SystemTime::now(),
        }
    }
}
