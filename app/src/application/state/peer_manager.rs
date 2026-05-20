use super::app_state::AppState;
use crate::domain::{EntryInfo, Peer, ServerEvent};
use std::{net::IpAddr, sync::Arc};
use tokio::sync::broadcast;
use tracing::info;
use uuid::Uuid;

/// Coordinates the live peer map on `AppState` and emits
/// `ServerEvent`s to the GUI when peers come and go.
pub struct PeerManager {
    state: Arc<AppState>,
    sse_tx: broadcast::Sender<ServerEvent>,
}

impl PeerManager {
    pub fn new(state: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            sse_tx: state.sse_sender(),
            state,
        })
    }

    /// Inserts or refreshes a peer. Emits `PeerConnected` only on the
    /// first appearance for a given `(id, instance_id)` pair, so a
    /// peer restart fires a fresh event while plain re-pings do not.
    #[tracing::instrument(skip_all, fields(peer = %peer.id, addr = %peer.addr))]
    pub async fn insert(&self, peer: Peer) {
        if !self.seen(&peer.id, &peer.instance_id).await {
            info!("Peer connected: {}", peer.id);
            self.send_sse_event(ServerEvent::PeerConnected {
                id: peer.id,
                addr: peer.addr,
                hostname: peer.hostname.clone(),
            })
            .await;
        }

        self.state.peers.write().await.insert(peer.id, peer);
    }

    /// Returns `true` if the manager already knows `id` and its
    /// `instance_id` matches — i.e. the peer has not restarted.
    pub async fn seen(&self, id: &Uuid, instance_id: &Uuid) -> bool {
        matches!(self.state.peers.read().await.get(id), Some(peer) if peer.instance_id == *instance_id)
    }

    pub async fn exists(&self, addr: IpAddr) -> bool {
        self.state
            .peers
            .read()
            .await
            .values()
            .any(|peer| peer.addr == addr)
    }

    pub async fn list(&self) -> Vec<Peer> {
        self.state.peers.read().await.values().cloned().collect()
    }

    /// Returns the addresses of peers that share the sync directory
    /// containing `entry`, i.e. the recipients of an outbound
    /// metadata broadcast for that entry.
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

    #[tracing::instrument(skip_all, fields(peer = %id))]
    pub async fn remove_peer(&self, id: Uuid) {
        if self.state.peers.write().await.remove(&id).is_some() {
            info!("Peer disconnected: {id}");
            self.send_sse_event(ServerEvent::PeerDisconnected(id)).await;
        }
    }

    #[tracing::instrument(skip_all, fields(addr = %addr))]
    pub async fn remove_peer_by_addr(&self, addr: IpAddr) {
        let mut peers = self.state.peers.write().await;

        if let Some(peer_id) = peers
            .iter()
            .find_map(|(id, peer)| (peer.addr == addr).then_some(*id))
        {
            peers.remove(&peer_id);
            info!("Peer disconnected: {peer_id}");
            self.send_sse_event(ServerEvent::PeerDisconnected(peer_id))
                .await;
        }
    }

    async fn send_sse_event(&self, event: ServerEvent) {
        if let Err(err) = self.sse_tx.send(event) {
            tracing::error!("Send Peer SSE error: {err}");
        }
    }
}
