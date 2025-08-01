use crate::domain::{EntryInfo, Peer};
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::RwLock,
    time::SystemTime,
};
use tracing::info;
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
            if !peers.contains_key(&peer.id) {
                info!("🟢 Peer connected: {}", peer.id);
            }
            peers.insert(peer.id, peer);
        }
    }

    pub fn insert_or_update(&self, id: Uuid, addr: IpAddr) -> bool {
        self.peers.write().is_ok_and(|mut peers| {
            if let Some(peer) = peers.get_mut(&id) {
                peer.last_seen = SystemTime::now();
                false
            } else {
                info!("🟢 Peer connected: {id}");
                peers.insert(id, Peer::new(id, addr, None));
                true
            }
        })
    }

    pub fn build_sync_map<'a>(
        &self,
        buffer: &'a HashMap<String, EntryInfo>,
    ) -> HashMap<IpAddr, Vec<&'a EntryInfo>> {
        let mut result = HashMap::new();

        if let Ok(peers) = self.peers.read() {
            for peer in peers.values() {
                for file in buffer.values() {
                    if peer.directories.contains_key(&file.get_root_parent()) {
                        result.entry(peer.addr).or_insert_with(Vec::new).push(file);
                    }
                }
            }
        }

        result
    }

    pub fn remove_peer(&self, id: Uuid) {
        if let Ok(mut peers) = self.peers.write() {
            info!("🔴 Peer disconnected: {id}");
            peers.remove(&id);
        }
    }

    pub fn retain(&self) -> Vec<String> {
        self.peers
            .write()
            .map(|mut peers| {
                let before: HashSet<Uuid> = peers.keys().cloned().collect();

                peers.retain(|_, peer| {
                    peer.last_seen
                        .elapsed()
                        .map(|e| e.as_secs() <= self.peer_timeout_secs)
                        .unwrap_or(true)
                });

                let after: HashSet<Uuid> = peers.keys().cloned().collect();

                // Log removed peers
                for removed in before.difference(&after) {
                    info!("🔴 Peer disconnected (timeout): {removed}");
                }

                after.iter().map(|k| k.to_string()).collect()
            })
            .unwrap_or_default()
    }
}
