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
                instance_id: peer.instance_id,
                last_seen: peer
                    .last_seen
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                sync_dirs: peer.sync_dirs.keys().cloned().collect(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RelativePath, SyncDirectory};
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::SystemTime;
    use tokio::sync::broadcast::error::TryRecvError;

    async fn setup() -> (
        crate::utils::test_support::TestEnv,
        Arc<PeerManager>,
        broadcast::Receiver<ServerEvent>,
    ) {
        let env = crate::utils::test_support::test_env().await;
        let sse_rx = env.state.sse_sender().subscribe();
        let pm = PeerManager::new(env.state.clone());
        (env, pm, sse_rx)
    }

    fn peer(id: Uuid, instance: Uuid, addr: IpAddr, dirs: Vec<&str>) -> Peer {
        Peer {
            id,
            instance_id: instance,
            addr,
            hostname: "host".into(),
            last_seen: SystemTime::now(),
            sync_dirs: dirs
                .into_iter()
                .map(|d| {
                    let rel: RelativePath = d.into();
                    (rel.clone(), SyncDirectory { name: rel })
                })
                .collect::<HashMap<_, _>>(),
        }
    }

    fn drain_connect(rx: &mut broadcast::Receiver<ServerEvent>) -> Option<Uuid> {
        loop {
            match rx.try_recv() {
                Ok(ServerEvent::PeerConnected { id, .. }) => return Some(id),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    fn drain_disconnect(rx: &mut broadcast::Receiver<ServerEvent>) -> Option<Uuid> {
        loop {
            match rx.try_recv() {
                Ok(ServerEvent::PeerDisconnected(id)) => return Some(id),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    #[tokio::test]
    async fn insert_emits_peer_connected_for_new_peer() {
        let (_env, pm, mut rx) = setup().await;
        let id = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        pm.insert(peer(id, Uuid::new_v4(), addr, vec![])).await;

        assert_eq!(drain_connect(&mut rx), Some(id));
        assert!(pm.exists(addr).await);
    }

    #[tokio::test]
    async fn insert_does_not_emit_event_for_refresh_of_known_peer() {
        let (_env, pm, mut rx) = setup().await;
        let id = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        pm.insert(peer(id, instance, addr, vec![])).await;
        assert_eq!(drain_connect(&mut rx), Some(id));

        // Re-insert with same (id, instance) — should be a silent refresh.
        pm.insert(peer(id, instance, addr, vec![])).await;
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn insert_emits_peer_connected_again_when_instance_id_changes() {
        let (_env, pm, mut rx) = setup().await;
        let id = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3));

        pm.insert(peer(id, Uuid::new_v4(), addr, vec![])).await;
        assert_eq!(drain_connect(&mut rx), Some(id));

        // Simulates a peer restart — same id, different instance_id.
        pm.insert(peer(id, Uuid::new_v4(), addr, vec![])).await;
        assert_eq!(drain_connect(&mut rx), Some(id));
    }

    #[tokio::test]
    async fn seen_returns_true_only_for_matching_id_and_instance_pair() {
        let (_env, pm, _rx) = setup().await;
        let id = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 4));

        pm.insert(peer(id, instance, addr, vec![])).await;

        assert!(pm.seen(&id, &instance).await);
        assert!(!pm.seen(&id, &Uuid::new_v4()).await);
        assert!(!pm.seen(&Uuid::new_v4(), &instance).await);
    }

    #[tokio::test]
    async fn remove_peer_emits_peer_disconnected_and_removes_from_state() {
        let (_env, pm, mut rx) = setup().await;
        let id = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));

        pm.insert(peer(id, Uuid::new_v4(), addr, vec![])).await;
        let _ = drain_connect(&mut rx);

        pm.remove_peer(id).await;
        assert_eq!(drain_disconnect(&mut rx), Some(id));
        assert!(!pm.exists(addr).await);
    }

    #[tokio::test]
    async fn remove_peer_by_addr_emits_disconnected_and_removes() {
        let (_env, pm, mut rx) = setup().await;
        let id = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 6));

        pm.insert(peer(id, Uuid::new_v4(), addr, vec![])).await;
        let _ = drain_connect(&mut rx);

        pm.remove_peer_by_addr(addr).await;
        assert_eq!(drain_disconnect(&mut rx), Some(id));
        assert!(!pm.exists(addr).await);
    }

    #[tokio::test]
    async fn remove_peer_by_addr_is_noop_for_unknown_addr() {
        let (_env, pm, mut rx) = setup().await;
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 99));

        pm.remove_peer_by_addr(addr).await;
        assert_eq!(drain_disconnect(&mut rx), None);
    }

    #[tokio::test]
    async fn get_peers_to_send_metadata_filters_by_sync_dir_membership() {
        let (_env, pm, _rx) = setup().await;
        let sharing = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 8));

        pm.insert(peer(
            Uuid::new_v4(),
            Uuid::new_v4(),
            sharing,
            vec!["Shared"],
        ))
        .await;
        pm.insert(peer(Uuid::new_v4(), Uuid::new_v4(), other, vec!["Other"]))
            .await;

        let entry = EntryInfo {
            name: "Shared/file.txt".into(),
            kind: crate::domain::EntryKind::File,
            hash: Some("h".into()),
            version: HashMap::new(),
        };

        let recipients = pm.get_peers_to_send_metadata(&entry).await;
        assert_eq!(recipients, vec![sharing]);
    }

    #[tokio::test]
    async fn peer_connected_event_includes_instance_last_seen_and_dirs() {
        let (_env, pm, mut rx) = setup().await;
        let id = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 10));

        pm.insert(peer(id, instance, addr, vec!["Docs"])).await;

        let event = rx.try_recv().unwrap();
        if let ServerEvent::PeerConnected {
            id: ev_id,
            instance_id,
            last_seen,
            sync_dirs,
            ..
        } = event
        {
            assert_eq!(ev_id, id);
            assert_eq!(instance_id, instance);
            assert!(last_seen > 0);
            assert_eq!(sync_dirs, vec![RelativePath::from("Docs")]);
        } else {
            panic!("expected PeerConnected");
        }
    }
}
