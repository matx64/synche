use crate::models::{entry::Entry, peer::Peer};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::RwLock,
};
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
        self.peers
            .read()
            .map(|peers| peers.get(ip).cloned())
            .unwrap_or_default()
    }

    pub fn insert_or_update(&self, peer: Peer) -> bool {
        let ip = peer.addr.ip();
        self.peers.write().is_ok_and(|mut peers| {
            let inserted = peers.insert(ip, peer).is_none();
            if inserted {
                info!("Peer connected: {}", ip);
            }
            inserted
        })
    }

    pub fn insert_entry(&self, ip: &IpAddr, entry: Entry) {
        if let Ok(mut peers) = self.peers.write() {
            if let Some(peer) = peers.get_mut(ip) {
                peer.entries.insert(entry.name.clone(), entry);
            }
        }
    }

    pub fn build_sync_map<'a>(
        &self,
        buffer: &'a HashMap<String, Entry>,
    ) -> HashMap<SocketAddr, Vec<&'a Entry>> {
        let mut result = HashMap::new();

        if let Ok(peers) = self.peers.read() {
            for peer in peers.values() {
                for entry in buffer.values() {
                    if let Some(peer_entry) = peer.entries.get(&entry.name) {
                        if !entry.is_dir
                            && !peer_entry.is_dir
                            && peer_entry.hash != entry.hash
                            && peer_entry.last_modified_at < entry.last_modified_at
                        {
                            result.entry(peer.addr).or_insert_with(Vec::new).push(entry);
                        }
                    }
                }
            }
        }

        result
    }

    pub fn retain(&self) -> String {
        self.peers
            .write()
            .map(|mut peers| {
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
            })
            .unwrap_or_default()
    }
}
