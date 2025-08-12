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

    pub fn get_peers_to_send_metadata(&self, entry: &EntryInfo) -> Vec<IpAddr> {
        let root_dir = entry.get_root_parent();

        self.peers
            .read()
            .map(|peers| {
                peers
                    .values()
                    .filter(|peer| peer.directories.contains_key(&root_dir))
                    .map(|peer| peer.addr)
                    .collect()
            })
            .unwrap_or_default()
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
