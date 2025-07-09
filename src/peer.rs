use crate::models::{entry::Entry, peer::Peer};
use std::{collections::HashMap, net::IpAddr, sync::RwLock};
use tracing::info;

pub struct PeerManager {
    peers: RwLock<HashMap<IpAddr, Peer>>,
    peer_timeout_secs: u64,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            peer_timeout_secs: 15,
        }
    }

    pub fn get(&self, ip: &IpAddr) -> Option<Peer> {
        if let Ok(peers) = self.peers.read() {
            peers.get(ip).cloned()
        } else {
            None
        }
    }

    pub fn insert_or_update(&self, peer: Peer) -> bool {
        let ip = peer.addr.ip();
        if let Ok(mut peers) = self.peers.write() {
            let inserted = peers.insert(ip, peer).is_none();
            if inserted {
                info!("Peer connected: {}", ip);
            }
            inserted
        } else {
            false
        }
    }

    pub fn insert_entry(&self, ip: &IpAddr, entry: Entry) {
        if let Ok(mut peers) = self.peers.write() {
            if let Some(peer) = peers.get_mut(ip) {
                peer.entries.insert(entry.name.clone(), entry);
            }
        }
    }

    pub fn find_peers_to_sync(&self, buffer: &HashMap<String, Entry>) -> Vec<Peer> {
        if let Ok(peers) = self.peers.read() {
            peers
                .values()
                .filter(|peer| {
                    buffer.values().any(|buf_entry| {
                        peer.entries
                            .get(&buf_entry.name)
                            .map(|peer_entry| {
                                peer_entry.hash != buf_entry.hash
                                    && peer_entry.last_modified_at < buf_entry.last_modified_at
                            })
                            .unwrap_or(false)
                    })
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    }

    pub fn retain(&self) -> String {
        if let Ok(mut peers) = self.peers.write() {
            peers.retain(|_, peer| {
                peer.last_seen
                    .elapsed()
                    .map(|e| e.as_secs() <= self.peer_timeout_secs)
                    .unwrap_or(true)
            });

            peers
                .keys()
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        }
    }
}
