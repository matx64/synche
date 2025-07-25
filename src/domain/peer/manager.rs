use crate::domain::{FileInfo, Peer};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::RwLock,
    time::SystemTime,
};
use uuid::Uuid;

pub struct PeerManager {
    peers: RwLock<HashMap<Uuid, Peer>>,
    peer_timeout_secs: u64,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            peer_timeout_secs: 15,
        }
    }

    pub fn list(&self) -> Vec<(Uuid, IpAddr)> {
        self.peers
            .read()
            .map(|peers| peers.values().map(|p| (p.id, p.addr.ip())).collect())
            .unwrap_or_default()
    }

    pub fn insert(&self, peer: Peer) {
        if let Ok(mut peers) = self.peers.write() {
            peers.insert(peer.id, peer);
        }
    }

    pub fn insert_or_update(&self, id: Uuid, addr: SocketAddr) -> bool {
        self.peers.write().is_ok_and(|mut peers| {
            if let Some(peer) = peers.get_mut(&id) {
                peer.last_seen = SystemTime::now();
                false
            } else {
                peers.insert(id, Peer::new(id, addr, None));
                true
            }
        })
    }

    pub fn build_sync_map<'a>(
        &self,
        buffer: &'a HashMap<String, FileInfo>,
    ) -> HashMap<SocketAddr, Vec<&'a FileInfo>> {
        let mut result = HashMap::new();

        if let Ok(peers) = self.peers.read() {
            for peer in peers.values() {
                for file in buffer.values() {
                    if peer.directories.contains_key(&file.get_dir()) {
                        result.entry(peer.addr).or_insert_with(Vec::new).push(file);
                    }
                }
            }
        }

        result
    }

    pub fn remove(&self, id: &Uuid) {
        if let Ok(mut peers) = self.peers.write() {
            peers.remove(id);
        }
    }
}
