use crate::{domain::Directory, proto::transport::PeerSyncData};
use std::{collections::HashMap, net::SocketAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: Uuid,
    pub addr: SocketAddr,
    pub directories: HashMap<String, Directory>,
    pub last_seen: SystemTime,
}

impl Peer {
    pub fn new(id: Uuid, addr: SocketAddr, data: Option<PeerSyncData>) -> Self {
        let directories = data
            .map(|data| {
                data.directories
                    .into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            id,
            addr,
            directories,
            last_seen: SystemTime::now(),
        }
    }
}
