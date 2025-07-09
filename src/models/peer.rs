use crate::models::entry::Entry;
use std::{collections::HashMap, net::SocketAddr, time::SystemTime};

#[derive(Debug, Clone)]
pub struct Peer {
    pub addr: SocketAddr,
    pub entries: HashMap<String, Entry>,
    pub last_seen: SystemTime,
}

impl Peer {
    pub fn new(addr: SocketAddr, entries: Option<HashMap<String, Entry>>) -> Self {
        Self {
            addr,
            entries: entries.unwrap_or_default(),
            last_seen: SystemTime::now(),
        }
    }
}
