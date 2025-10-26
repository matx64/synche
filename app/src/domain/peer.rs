use crate::domain::{RelativePath, SyncDirectory};
use serde::Serialize;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub hostname: String,
    pub last_seen: SystemTime,
    pub sync_dirs: HashMap<RelativePath, SyncDirectory>,
}

impl Peer {
    pub fn new(id: Uuid, addr: IpAddr, hostname: String, dirs: Option<Vec<SyncDirectory>>) -> Self {
        let sync_dirs = dirs
            .map(|dirs| {
                dirs.into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect::<HashMap<RelativePath, SyncDirectory>>()
            })
            .unwrap_or_default();

        Self {
            id,
            addr,
            hostname,
            sync_dirs,
            last_seen: SystemTime::now(),
        }
    }
}
