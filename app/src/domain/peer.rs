use crate::domain::{RelativePath, SyncDirectory};
use serde::Serialize;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub hostname: String,
    pub instance_id: Uuid,
    pub last_seen: SystemTime,
    pub sync_dirs: HashMap<RelativePath, SyncDirectory>,
}

impl Peer {
    pub fn new(id: Uuid, addr: IpAddr, hostname: String, instance_id: Uuid, sync_dirs: Vec<SyncDirectory>) -> Self {
        let hostname = hostname.strip_suffix(".local").unwrap_or(&hostname).to_string();
        let sync_dirs = sync_dirs.into_iter().map(|dir| (dir.name.clone(), dir)).collect();

        Self {
            id,
            instance_id,
            addr,
            hostname,
            sync_dirs,
            last_seen: SystemTime::now(),
        }
    }
}
