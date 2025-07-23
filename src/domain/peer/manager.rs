use crate::domain::{FileInfo, Peer};
use std::{collections::HashMap, net::SocketAddr, sync::RwLock, time::SystemTime};
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

    pub fn retain(&self) -> Vec<String> {
        self.peers
            .write()
            .map(|mut peers| {
                peers.retain(|_, peer| {
                    peer.last_seen
                        .elapsed()
                        .map(|e| e.as_secs() <= self.peer_timeout_secs)
                        .unwrap_or(true)
                });

                peers.keys().map(|k| k.to_string()).collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }
}
