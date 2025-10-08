use crate::domain::SyncDirectory;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub sync_directories: HashMap<String, SyncDirectory>,
    pub last_seen: SystemTime,
}

impl Peer {
    pub fn new(id: Uuid, addr: IpAddr, dirs: Option<Vec<SyncDirectory>>) -> Self {
        let sync_directories = dirs
            .map(|dirs| {
                dirs.into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect::<HashMap<String, SyncDirectory>>()
            })
            .unwrap_or_default();

        Self {
            id,
            addr,
            sync_directories,
            last_seen: SystemTime::now(),
        }
    }
}
