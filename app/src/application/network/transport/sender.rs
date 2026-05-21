use crate::{
    application::{
        AppState, EntryManager, PeerManager, network::transport::interface::TransportInterface,
        persistence::interface::PersistenceInterface,
    },
    domain::{EntryInfo, MutexChannel, TransportChannelData, TransportData},
    utils::fs::is_git_path,
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{
    io,
    sync::{Mutex, mpsc::Receiver},
};
use tracing::{error, warn};

/// Outbound side of the transport service.
///
/// Reads `TransportChannelData` items off the shared outbound channel
/// and splits them across two priority lanes — `control_chan` for
/// handshakes/metadata/requests, `transfer_chan` for bulk entry
/// transfers — so the latter cannot delay protocol messages.
pub struct TransportSender<T: TransportInterface, P: PersistenceInterface> {
    adapter: Arc<T>,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_rx: Mutex<Receiver<TransportChannelData>>,
    control_chan: MutexChannel<TransportChannelData>,
    transfer_chan: MutexChannel<(IpAddr, EntryInfo)>,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportSender<T, P> {
    pub fn new(
        adapter: Arc<T>,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        send_rx: Mutex<Receiver<TransportChannelData>>,
    ) -> Self {
        Self {
            state,
            adapter,
            peer_manager,
            entry_manager,
            send_rx,
            control_chan: MutexChannel::new(100),
            transfer_chan: MutexChannel::new(16),
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::select!(
            res = self.send() => res,
            res = self.send_control() => res,
            res = self.send_files() => res
        )
    }

    async fn send(&self) -> io::Result<()> {
        while let Some(data) = self.send_rx.lock().await.recv().await {
            match data {
                TransportChannelData::Transfer(data) => {
                    self.transfer_chan
                        .tx
                        .send(data)
                        .await
                        .map_err(io::Error::other)?;
                }

                _ => {
                    self.control_chan
                        .tx
                        .send(data)
                        .await
                        .map_err(io::Error::other)?;
                }
            }
        }
        warn!("Transport Send channel closed");
        Ok(())
    }

    async fn send_control(&self) -> io::Result<()> {
        while let Some(data) = self.control_chan.recv().await {
            match data {
                TransportChannelData::HandshakeSyn(target) => {
                    self.send_handshake(target, true).await?;
                }

                TransportChannelData::_HandshakeAck(target) => {
                    self.send_handshake(target, false).await?;
                }

                TransportChannelData::Metadata(entry) => {
                    self.send_metadata(entry).await?;
                }

                TransportChannelData::Request((target, entry)) => {
                    self.send_request(target, entry).await?;
                }

                _ => unreachable!(),
            }
        }
        warn!("Transport Send Control channel closed");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(target = %target, is_syn))]
    async fn send_handshake(&self, target: IpAddr, is_syn: bool) -> io::Result<()> {
        let data = self.entry_manager.get_handshake_data().await?;

        self.try_send(
            || {
                let data = if is_syn {
                    TransportData::HandshakeSyn(data.clone())
                } else {
                    TransportData::HandshakeAck(data.clone())
                };

                self.adapter.send(target, data).map_err(|e| e.into())
            },
            target,
        )
        .await;

        Ok(())
    }

    #[tracing::instrument(skip_all, fields(entry = %entry.name))]
    async fn send_metadata(&self, entry: EntryInfo) -> io::Result<()> {
        if is_git_path(&entry.name) {
            return Ok(());
        }

        for target in self.peer_manager.get_peers_to_send_metadata(&entry).await {
            self.try_send(
                || {
                    self.adapter
                        .send(target, TransportData::Metadata(entry.clone()))
                        .map_err(|e| e.into())
                },
                target,
            )
            .await;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(target = %target, entry = %entry.name))]
    async fn send_request(&self, target: IpAddr, entry: EntryInfo) -> io::Result<()> {
        if is_git_path(&entry.name) {
            return Ok(());
        }

        self.try_send(
            || {
                self.adapter
                    .send(target, TransportData::Request(entry.clone()))
                    .map_err(|e| e.into())
            },
            target,
        )
        .await;
        Ok(())
    }

    async fn send_files(&self) -> io::Result<()> {
        while let Some((target, entry)) = self.transfer_chan.recv().await {
            if is_git_path(&entry.name) {
                continue;
            }

            let path = entry.name.to_canonical(self.state.home_path());

            if !path.exists() || !path.is_file() {
                continue;
            }

            self.try_send(
                || {
                    self.adapter
                        .send(target, TransportData::Transfer(entry.clone()))
                        .map_err(|e| e.into())
                },
                target,
            )
            .await;
        }
        warn!("Transport Transfer channel closed");
        Ok(())
    }

    async fn try_send<F, Fut>(&self, mut op: F, addr: IpAddr)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = io::Result<()>>,
    {
        for _ in 0..3 {
            if let Err(err) = op().await {
                error!(peer = ?addr, "Transport send error: {err}");
            } else {
                return;
            }

            if !self.peer_manager.exists(addr).await {
                warn!("cancelled transport send: peer disconnected mid-op");
                return;
            }
        }

        error!(peer = ?addr, "Disconnecting peer after 3 Transport send attempts.");
        self.peer_manager.remove_peer_by_addr(addr).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::network::transport::test_support::RecordingTransport,
        domain::{EntryInfo, EntryKind, Peer, SyncDirectory},
        infra::persistence::sqlite::SqliteDb,
    };
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
    };
    use uuid::Uuid;

    struct Harness {
        _env: crate::utils::test_support::TestEnv,
        sender: TransportSender<RecordingTransport, SqliteDb>,
        peer_manager: Arc<PeerManager>,
        adapter: Arc<RecordingTransport>,
    }

    async fn setup() -> Harness {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let db = SqliteDb::new(":memory:").await.unwrap();
        let entry_manager = EntryManager::new(db, state.clone());
        let peer_manager = PeerManager::new(state.clone());

        let adapter = Arc::new(RecordingTransport::new());
        let (_send_tx, send_rx) = tokio::sync::mpsc::channel(8);

        let sender = TransportSender::new(
            adapter.clone(),
            state,
            peer_manager.clone(),
            entry_manager,
            Mutex::new(send_rx),
        );

        Harness {
            _env: env,
            sender,
            peer_manager,
            adapter,
        }
    }

    async fn add_peer(pm: &PeerManager, addr: IpAddr, sync_dirs: Vec<SyncDirectory>) -> Uuid {
        let id = Uuid::new_v4();
        pm.insert(Peer::new(
            id,
            addr,
            "host".into(),
            Uuid::new_v4(),
            sync_dirs,
        ))
        .await;
        id
    }

    fn entry(name: &str) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::File,
            hash: Some("h".into()),
            version: HashMap::from([(Uuid::new_v4(), 1)]),
        }
    }

    /// A `send_handshake` call reaches the adapter as exactly one `send`
    /// against the target address.
    #[tokio::test]
    async fn send_handshake_dispatches_handshake_syn_to_target() {
        let h = setup().await;
        let target = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        h.sender.send_handshake(target, true).await.unwrap();

        let recorded = h.adapter.sends.lock().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, target);
        assert!(matches!(
            recorded[0].1,
            crate::domain::TransportData::HandshakeSyn(_)
        ));
    }

    /// Git paths must be filtered out before any outbound send — the
    /// transport contract excludes `.git/` entries at every boundary.
    #[tokio::test]
    async fn send_metadata_skips_git_paths() {
        let h = setup().await;
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        add_peer(
            &h.peer_manager,
            addr,
            vec![SyncDirectory {
                name: "sync".into(),
            }],
        )
        .await;

        h.sender
            .send_metadata(entry("sync/.git/config"))
            .await
            .unwrap();

        assert_eq!(h.adapter.sends_count().await, 0);
    }

    /// `send_request` must also drop git paths — covers the second
    /// boundary where they could otherwise leak.
    #[tokio::test]
    async fn send_request_skips_git_paths() {
        let h = setup().await;
        let target = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 6));

        h.sender
            .send_request(target, entry("sync/.git/HEAD"))
            .await
            .unwrap();

        assert_eq!(h.adapter.sends_count().await, 0);
    }

    /// Metadata broadcasts only to peers that share the entry's
    /// top-level sync directory.
    #[tokio::test]
    async fn send_metadata_broadcasts_only_to_peers_sharing_sync_dir() {
        let h = setup().await;
        let sharing = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 4));

        add_peer(
            &h.peer_manager,
            sharing,
            vec![SyncDirectory {
                name: "Default Folder".into(),
            }],
        )
        .await;
        add_peer(
            &h.peer_manager,
            other,
            vec![SyncDirectory {
                name: "Other Dir".into(),
            }],
        )
        .await;

        h.sender
            .send_metadata(entry("Default Folder/file.txt"))
            .await
            .unwrap();

        let recorded = h.adapter.sends.lock().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, sharing);
    }

    /// Three consecutive `send` failures must evict the peer via
    /// `PeerManager::remove_peer_by_addr` — the disconnect contract that
    /// keeps a dead TCP target from blocking the sender forever.
    #[tokio::test]
    async fn send_disconnects_peer_after_three_consecutive_failures() {
        let h = setup().await;
        let addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        add_peer(&h.peer_manager, addr, vec![]).await;
        h.adapter.set_fail_sends(true);

        h.sender.send_handshake(addr, true).await.unwrap();

        assert!(
            !h.peer_manager.exists(addr).await,
            "peer should have been evicted after 3 failed sends"
        );
        assert_eq!(
            h.adapter.sends_count().await,
            0,
            "no send should have succeeded"
        );
    }
}
