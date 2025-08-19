use crate::domain::Directory;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub sync_directories: HashMap<String, Directory>,
    pub last_seen: SystemTime,
}

impl Peer {
    pub fn new(id: Uuid, addr: IpAddr, dirs: Option<Vec<Directory>>) -> Self {
        let sync_directories = dirs
            .map(|dirs| {
                dirs.into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect::<HashMap<String, Directory>>()
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
