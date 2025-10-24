use crate::domain::{AppState, EntryInfo, Peer};
use std::{net::IpAddr, sync::Arc, time::SystemTime};
use tracing::info;
use uuid::Uuid;

pub struct PeerManager {
    state: Arc<AppState>,
}

impl PeerManager {
    pub fn new(state: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self { state })
    }

    pub async fn insert(&self, peer: Peer) {
        let mut peers = self.state.peers.write().await;

        if !peers.contains_key(&peer.id) {
            info!("🟢 Peer connected: {}", peer.id);
        }

        peers.insert(peer.id, peer);
    }

    pub async fn insert_or_update(&self, id: Uuid, addr: IpAddr, hostname: String) -> bool {
        let mut peers = self.state.peers.write().await;

        if let Some(peer) = peers.get_mut(&id) {
            peer.last_seen = SystemTime::now();
            false
        } else {
            info!("🟢 Peer connected: {id}");
            peers.insert(id, Peer::new(id, addr, hostname, None));
            true
        }
    }

    pub async fn exists(&self, addr: IpAddr) -> bool {
        self.state
            .peers
            .read()
            .await
            .values()
            .any(|peer| peer.addr == addr)
    }

    pub async fn list(&self) -> Vec<(Uuid, IpAddr)> {
        self.state
            .peers
            .read()
            .await
            .values()
            .map(|p| (p.id, p.addr))
            .collect()
    }

    pub async fn get_peers_to_send_metadata(&self, entry: &EntryInfo) -> Vec<IpAddr> {
        let root_dir = entry.get_sync_dir();

        self.state
            .peers
            .read()
            .await
            .values()
            .filter(|peer| peer.sync_dirs.contains_key(&root_dir))
            .map(|peer| peer.addr)
            .collect()
    }

    pub async fn remove_peer(&self, id: Uuid) {
        if self.state.peers.write().await.remove(&id).is_some() {
            info!("🔴 Peer disconnected: {id}");
        }
    }

    pub async fn remove_peer_by_addr(&self, addr: IpAddr) {
        let mut peers = self.state.peers.write().await;

        if let Some(peer_id) = peers
            .iter()
            .find_map(|(id, peer)| (peer.addr == addr).then_some(*id))
        {
            peers.remove(&peer_id);
            info!("🔴 Peer disconnected: {peer_id}");
        }
    }
}
