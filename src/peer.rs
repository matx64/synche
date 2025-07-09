use crate::models::{entry::Entry, peer::Peer};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::RwLock,
    time::SystemTime,
};

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

    pub fn insert(&self, peer: Peer) {
        if let Ok(mut peers) = self.peers.write() {
            peers.insert(peer.addr.ip(), peer);
        }
    }

    pub fn insert_or_update(&self, addr: SocketAddr) -> bool {
        let ip = addr.ip();

        self.peers.write().is_ok_and(|mut peers| {
            if let Some(peer) = peers.get_mut(&ip) {
                peer.last_seen = SystemTime::now();
                false
            } else {
                peers.insert(ip, Peer::new(addr, None));
                true
            }
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
