use crate::domain::SyncDirectory;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub _hostname: String,
    pub last_seen: SystemTime,
    pub sync_dirs: HashMap<String, SyncDirectory>,
}

impl Peer {
    pub fn new(id: Uuid, addr: IpAddr, hostname: String, dirs: Option<Vec<SyncDirectory>>) -> Self {
        let sync_dirs = dirs
            .map(|dirs| {
                dirs.into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect::<HashMap<String, SyncDirectory>>()
            })
            .unwrap_or_default();

        Self {
            id,
            addr,
            _hostname: hostname,
            sync_dirs,
            last_seen: SystemTime::now(),
        }
    }
}
