use crate::domain::Directory;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub directories: HashMap<String, Directory>,
    pub last_seen: SystemTime,
}

impl Peer {
    pub fn new(id: Uuid, addr: IpAddr, dirs: Option<Vec<Directory>>) -> Self {
        let directories = dirs
            .map(|dirs| {
                dirs.into_iter()
                    .map(|d| (d.name.clone(), d))
                    .collect::<HashMap<String, Directory>>()
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
