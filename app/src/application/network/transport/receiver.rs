use crate::{
    application::{
        AppState, EntryManager, PeerManager, network::transport::interface::TransportInterface,
        persistence::interface::PersistenceInterface, state::entry_manager::CommitOutcome,
    },
    domain::{
        EntryInfo, MutexChannel, Peer, RelativePath, ServerEvent, TransportChannelData,
        TransportData, TransportEvent, VersionCmp,
    },
    utils::fs::is_git_path,
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::mpsc::Sender};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Inbound side of the transport service.
///
/// Pulls events off the adapter and dispatches them onto two internal
/// queues — `control_chan` for handshake/metadata/request messages
/// and `transfer_chan` for entry bytes — so a slow file write cannot
/// starve protocol traffic. Handlers reconcile peer state, persist
/// metadata, and either accept, write, or conflict-resolve incoming
/// entries before re-broadcasting.
pub struct TransportReceiver<T: TransportInterface, P: PersistenceInterface> {
    adapter: Arc<T>,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_tx: Sender<TransportChannelData>,
    control_chan: MutexChannel<TransportEvent>,
    transfer_chan: MutexChannel<TransportEvent>,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportReceiver<T, P> {
    pub fn new(
        adapter: Arc<T>,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        send_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            adapter,
            state,
            peer_manager,
            entry_manager,
            send_tx,
            control_chan: MutexChannel::new(100),
            transfer_chan: MutexChannel::new(16),
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::select!(
            res = self.recv() => res,
            res = self.recv_control() => res,
            res = self.recv_transfer() => res
        )
    }

    async fn recv(&self) -> io::Result<()> {
        loop {
            let event = self.adapter.recv().await?;
            match event.payload {
                TransportData::Transfer(_) => {
                    self.transfer_chan
                        .tx
                        .send(event)
                        .await
                        .map_err(io::Error::other)?;
                }

                _ => {
                    self.control_chan
                        .tx
                        .send(event)
                        .await
                        .map_err(io::Error::other)?;
                }
            }
        }
    }

    async fn recv_transfer(&self) -> io::Result<()> {
        while let Some(event) = self.transfer_chan.recv().await {
            self.handle_transfer(event).await?;
        }
        warn!("Transport RECV Transfer channel closed");
        Ok(())
    }

    async fn recv_control(&self) -> io::Result<()> {
        while let Some(event) = self.control_chan.recv().await {
            match event.payload {
                TransportData::HandshakeSyn(_) | TransportData::HandshakeAck(_) => {
                    self.handle_handshake(event).await?;
                }

                TransportData::Metadata(_) => {
                    self.handle_metadata(event).await?;
                }

                TransportData::Request(_) => {
                    self.handle_request(event).await?;
                }

                _ => unreachable!(),
            }
        }
        warn!("Transport RECV Control channel closed");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(peer = %event.metadata.source_id))]
    async fn handle_handshake(&self, event: TransportEvent) -> io::Result<()> {
        let (hs_data, is_syn) = match event.payload {
            TransportData::HandshakeSyn(data) => (data, true),
            TransportData::HandshakeAck(data) => (data, false),
            _ => unreachable!(),
        };

        let peer = Peer::new(
            event.metadata.source_id,
            event.metadata.source_ip,
            hs_data.hostname,
            hs_data.instance_id,
            hs_data.sync_dirs,
        );
        self.peer_manager.insert(peer.clone()).await;

        if is_syn {
            // Can't use send_tx because Response must be sent strictly BEFORE syncing
            let data = self.entry_manager.get_handshake_data().await?;
            self.try_send(
                || {
                    self.adapter
                        .send(peer.addr, TransportData::HandshakeAck(data.clone()))
                        .map_err(|e| e.into())
                },
                peer.addr,
            )
            .await;
        }

        info!(peer = ?peer.id, "syncing peer");

        let entries_to_request = self
            .entry_manager
            .get_entries_to_request(&peer, hs_data.entries)
            .await?;

        for entry in entries_to_request {
            if entry.is_removed() {
                self.remove_entry(&entry.name).await?;
            } else if entry.is_file() {
                // Issue #33 B1: register the outstanding request BEFORE
                // enqueuing it on the wire so the matching Transfer is
                // recognized as solicited.
                self.state
                    .register_pending_request(peer.id, entry.name.clone())
                    .await;
                self.broadcast_sync_started(event.metadata.source_id, &entry);
                self.send_tx
                    .send(TransportChannelData::Request((peer.addr, entry)))
                    .await
                    .map_err(io::Error::other)?;
            } else {
                self.create_received_dir(event.metadata.source_id, entry)
                    .await?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(peer = %event.metadata.source_id))]
    async fn handle_metadata(&self, event: TransportEvent) -> io::Result<()> {
        let peer_entry = match event.payload {
            TransportData::Metadata(entry) => entry,
            _ => unreachable!(),
        };

        if is_git_path(&peer_entry.name) || !self.is_in_configured_sync_dir(&peer_entry).await {
            return Ok(());
        }

        match self
            .entry_manager
            .handle_metadata(event.metadata.source_id, &peer_entry)
            .await?
        {
            VersionCmp::KeepOther => {
                if peer_entry.is_removed() {
                    self.remove_entry(&peer_entry.name).await
                } else if peer_entry.is_file() {
                    // Issue #33 B1: register the outstanding request so
                    // the matching Transfer is recognized as solicited
                    // before any bytes hit disk.
                    self.state
                        .register_pending_request(event.metadata.source_id, peer_entry.name.clone())
                        .await;
                    self.broadcast_sync_started(event.metadata.source_id, &peer_entry);
                    self.send_tx
                        .send(TransportChannelData::Request((
                            event.metadata.source_ip,
                            peer_entry,
                        )))
                        .await
                        .map_err(io::Error::other)
                } else {
                    self.create_received_dir(event.metadata.source_id, peer_entry)
                        .await
                }
            }

            _ => Ok(()),
        }
    }

    #[tracing::instrument(skip_all, fields(peer = %event.metadata.source_id))]
    async fn handle_request(&self, event: TransportEvent) -> io::Result<()> {
        let requested_entry = match event.payload {
            TransportData::Request(entry) => entry,
            _ => unreachable!(),
        };

        if is_git_path(&requested_entry.name)
            || !self.is_in_configured_sync_dir(&requested_entry).await
        {
            return Ok(());
        }

        match self.entry_manager.get_entry(&requested_entry.name).await? {
            Some(local_entry)
                if local_entry.is_file()
                    && matches!(local_entry.compare(&requested_entry), VersionCmp::Equal) =>
            {
                self.send_tx
                    .send(TransportChannelData::Transfer((
                        event.metadata.source_ip,
                        local_entry,
                    )))
                    .await
                    .map_err(io::Error::other)
            }

            _ => Ok(()),
        }
    }

    #[tracing::instrument(skip_all, fields(peer = %event.metadata.source_id))]
    async fn handle_transfer(&self, event: TransportEvent) -> io::Result<()> {
        let TransportEvent {
            payload,
            metadata,
            staging,
        } = event;
        let received_entry = match payload {
            TransportData::Transfer(entry) => entry,
            _ => unreachable!(),
        };
        let peer_id = metadata.source_id;

        // Same scope guards the TCP layer already enforced — defense in
        // depth at the application boundary. Dropping `staging` here
        // cleans up the tmp dir via its RAII guard.
        if is_git_path(&received_entry.name)
            || !self.is_in_configured_sync_dir(&received_entry).await
        {
            return Ok(());
        }

        // Issue #33 B1: every Transfer must be backed by an outstanding
        // Request we sent. Unsolicited transfers are dropped without
        // touching home_path. `staging` is the RAII handle on the
        // staged bytes — let it drop on every failure path.
        if !self
            .state
            .take_pending_request(peer_id, &received_entry.name)
            .await
        {
            warn!(
                peer = %peer_id,
                entry = %received_entry.name,
                "dropping unsolicited Transfer"
            );
            self.broadcast_sync_failed_reason(
                peer_id,
                &received_entry.name,
                "unsolicited transfer",
            );
            return Ok(());
        }

        let Some(staging) = staging else {
            warn!(
                peer = %peer_id,
                entry = %received_entry.name,
                "dropping Transfer without staged bytes"
            );
            self.broadcast_sync_failed_reason(
                peer_id,
                &received_entry.name,
                "transfer without staged bytes",
            );
            return Ok(());
        };

        // Per-entry serialization across the compare → rename →
        // persist commit.
        let entry_name = received_entry.name.clone();
        let lock = self.state.acquire_inflight_lock(&entry_name).await;
        let outcome = {
            let _guard = lock.lock().await;
            self.entry_manager
                .commit_staged_transfer(peer_id, received_entry, staging)
                .await?
        };
        drop(lock);
        self.state.release_inflight_lock(&entry_name).await;

        match outcome {
            CommitOutcome::Committed(entry) => {
                self.broadcast_sync_completed(peer_id, &entry);
                self.send_tx
                    .send(TransportChannelData::Metadata(entry))
                    .await
                    .map_err(io::Error::other)
            }
            CommitOutcome::Dropped(reason) => {
                warn!(peer = %peer_id, entry = %entry_name, reason, "dropping staged Transfer");
                self.broadcast_sync_failed_reason(peer_id, &entry_name, reason);
                Ok(())
            }
        }
    }

    /// Returns true if `entry`'s top-level component is one of the
    /// directories the local user has opted in to syncing. Acts as a
    /// scope guard for inbound Metadata / Request / Transfer so a peer
    /// cannot push or pull data outside the configured sync set.
    async fn is_in_configured_sync_dir(&self, entry: &EntryInfo) -> bool {
        self.state.contains_sync_dir(&entry.get_sync_dir()).await
    }

    fn broadcast_sync_started(&self, peer: Uuid, entry: &EntryInfo) {
        let _ = self.state.sse_sender().send(ServerEvent::EntrySyncStarted {
            dir: entry.get_sync_dir(),
            relative_path: entry.name.clone(),
            peer,
        });
    }

    fn broadcast_sync_completed(&self, peer: Uuid, entry: &EntryInfo) {
        let _ = self
            .state
            .sse_sender()
            .send(ServerEvent::EntrySyncCompleted {
                dir: entry.get_sync_dir(),
                relative_path: entry.name.clone(),
                peer,
            });
    }

    fn broadcast_sync_failed_reason(&self, peer: Uuid, entry_name: &RelativePath, reason: &str) {
        let _ = self.state.sse_sender().send(ServerEvent::EntrySyncFailed {
            dir: entry_name.sync_dir(),
            relative_path: entry_name.clone(),
            peer,
            reason: reason.to_string(),
        });
    }

    async fn create_received_dir(&self, peer_id: Uuid, dir: EntryInfo) -> io::Result<()> {
        let Some(dir) = self.entry_manager.insert_peer_entry(peer_id, dir).await? else {
            return Ok(());
        };

        let path = dir.name.to_canonical(self.state.home_path());
        fs::create_dir_all(path).await?;

        self.send_tx
            .send(TransportChannelData::Metadata(dir))
            .await
            .map_err(io::Error::other)
    }

    async fn remove_entry(&self, entry_name: &str) -> io::Result<()> {
        let _ = self.entry_manager.remove_entry(entry_name).await?;

        let path = self.state.home_path().join(entry_name);

        if path.is_dir() {
            fs::remove_dir_all(path).await?;
        } else if path.is_file() {
            fs::remove_file(path).await?;
        }
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
        domain::{EntryKind, HandshakeData, StagedTransfer, TransportMetadata},
        infra::persistence::sqlite::SqliteDb,
    };
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
        path::PathBuf,
    };
    use tokio::sync::mpsc::error::TryRecvError;
    use uuid::Uuid;

    /// Build a `StagedTransfer` containing `contents`. The returned
    /// guard cleans up the staging dir on drop unless the application
    /// commits the file out. The staging root is rooted under the
    /// per-test temp dir to keep concurrent tests isolated.
    async fn make_staged_transfer(
        env: &crate::utils::test_support::TestEnv,
        contents: &[u8],
    ) -> StagedTransfer {
        let root: PathBuf = env
            .home_path()
            .join(format!(".staging-{}", Uuid::new_v4()))
            .to_path_buf();
        tokio::fs::create_dir_all(&root).await.unwrap();
        let path = root.join("payload.bin");
        tokio::fs::write(&path, contents).await.unwrap();
        StagedTransfer::new(path, root)
    }

    async fn setup() -> (
        crate::utils::test_support::TestEnv,
        TransportReceiver<RecordingTransport, SqliteDb>,
        Arc<EntryManager<SqliteDb>>,
        tokio::sync::mpsc::Receiver<TransportChannelData>,
    ) {
        // Use "sync" as the configured directory so the existing
        // `sync/...` entry paths are inside a configured sync dir
        // (scope guard added for issue #32).
        let env = crate::utils::test_support::test_env_with_dirs(&["sync"]).await;
        let state = env.state.clone();
        let db = SqliteDb::new(":memory:").await.unwrap();
        let entry_manager = EntryManager::new(db, state.clone());
        let peer_manager = PeerManager::new(state.clone());
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(4);
        let receiver = TransportReceiver::new(
            Arc::new(RecordingTransport::new()),
            state,
            peer_manager,
            entry_manager.clone(),
            send_tx,
        );

        (env, receiver, entry_manager, send_rx)
    }

    fn git_entry(name: &str) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::File,
            hash: Some("hash".to_string()),
            version: HashMap::from([(Uuid::new_v4(), 1)]),
        }
    }

    fn event(payload: TransportData) -> TransportEvent {
        TransportEvent {
            payload,
            metadata: TransportMetadata {
                source_id: Uuid::new_v4(),
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: None,
        }
    }

    #[tokio::test]
    async fn handle_request_ignores_git_entries_without_enqueuing_transfer() {
        let (_env, receiver, entry_manager, mut send_rx) = setup().await;
        let entry = git_entry("sync/.git/config");
        entry_manager.insert_entry(entry.clone()).await.unwrap();

        receiver
            .handle_request(event(TransportData::Request(entry)))
            .await
            .unwrap();

        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn handle_transfer_ignores_git_entries_without_inserting_metadata() {
        let (_env, receiver, entry_manager, mut send_rx) = setup().await;
        let entry = git_entry("sync/.git/config");

        receiver
            .handle_transfer(event(TransportData::Transfer(entry.clone())))
            .await
            .unwrap();

        assert!(
            entry_manager
                .get_entry(&entry.name)
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    fn file_entry(name: &str) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::File,
            hash: Some("hash".to_string()),
            version: HashMap::from([(Uuid::new_v4(), 1)]),
        }
    }

    #[tokio::test]
    async fn handle_handshake_applies_tombstone_without_requesting_transfer() {
        let (env, receiver, entry_manager, mut send_rx) = setup().await;
        let local_id = env.state.local_id();
        let peer = Uuid::new_v4();
        let name: RelativePath = "sync/deleted.txt".into();
        let home_file = env.home_path().join(&*name);
        tokio::fs::create_dir_all(home_file.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&home_file, b"local live copy")
            .await
            .unwrap();

        entry_manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("local-live".to_string()),
                version: HashMap::from([(local_id, 0)]),
            })
            .await
            .unwrap();

        let mut tombstone = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: None,
            version: HashMap::from([(peer, 2)]),
        };
        tombstone.set_removed_hash();

        let evt = TransportEvent {
            payload: TransportData::HandshakeAck(HandshakeData {
                hostname: "remote".to_string(),
                instance_id: Uuid::new_v4(),
                sync_dirs: Vec::new(),
                entries: HashMap::from([(name.clone(), tombstone)]),
            }),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: None,
        };

        receiver.handle_handshake(evt).await.unwrap();

        assert!(
            !home_file.exists(),
            "remote tombstone from handshake must remove the local file"
        );
        let stored = entry_manager
            .get_entry(&name)
            .await
            .unwrap()
            .expect("delete must leave a durable tombstone");
        assert!(stored.is_removed());
        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn handle_transfer_emits_sync_completed_after_commit() {
        let (env, receiver, _entry_manager, _send_rx) = setup().await;
        let mut sse_rx = env.state.sse_subscribe();
        let peer = Uuid::new_v4();
        let entry = file_entry("sync/payload.bin");
        let contents = b"committed bytes";

        env.state
            .register_pending_request(peer, entry.name.clone())
            .await;
        let staging = make_staged_transfer(&env, contents).await;

        let evt = TransportEvent {
            payload: TransportData::Transfer(entry.clone()),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: Some(staging),
        };
        receiver.handle_transfer(evt).await.unwrap();

        match sse_rx.try_recv().expect("expected EntrySyncCompleted") {
            ServerEvent::EntrySyncCompleted {
                dir,
                relative_path,
                peer: emitted_peer,
            } => {
                assert_eq!(dir.as_ref() as &str, "sync");
                assert_eq!(relative_path, entry.name);
                assert_eq!(emitted_peer, peer);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        // The committed file must be on disk at home_path with the
        // staged bytes.
        let on_disk = tokio::fs::read(env.home_path().join(&*entry.name))
            .await
            .unwrap();
        assert_eq!(on_disk, contents);
    }

    #[tokio::test]
    async fn handle_metadata_drops_entries_outside_configured_sync_dirs() {
        let (_env, receiver, entry_manager, mut send_rx) = setup().await;
        // "other" is not in the configured sync_dirs (only "sync" is).
        let entry = file_entry("other/payload.bin");

        receiver
            .handle_metadata(event(TransportData::Metadata(entry.clone())))
            .await
            .unwrap();

        assert!(
            entry_manager
                .get_entry(&entry.name)
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn handle_request_drops_entries_outside_configured_sync_dirs() {
        let (_env, receiver, entry_manager, mut send_rx) = setup().await;
        let entry = file_entry("other/payload.bin");
        // Force-insert the entry to prove the scope guard rejects the
        // request before any DB lookup or transfer enqueue.
        entry_manager.insert_entry(entry.clone()).await.unwrap();

        receiver
            .handle_request(event(TransportData::Request(entry)))
            .await
            .unwrap();

        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn handle_transfer_strips_foreign_axes_from_peer_version_vector() {
        let (env, receiver, entry_manager, _send_rx) = setup().await;
        let peer = Uuid::new_v4();
        let third = Uuid::new_v4();
        let entry = EntryInfo {
            name: "sync/payload.bin".into(),
            kind: EntryKind::File,
            hash: Some("hash".into()),
            // Peer reports its own axis AND a claim about `third`'s
            // counter — only the peer's own axis must be persisted.
            version: HashMap::from([(peer, 3), (third, 99)]),
        };

        env.state
            .register_pending_request(peer, entry.name.clone())
            .await;
        let staging = make_staged_transfer(&env, b"payload").await;

        let evt = TransportEvent {
            payload: TransportData::Transfer(entry.clone()),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: Some(staging),
        };
        receiver.handle_transfer(evt).await.unwrap();

        let stored = entry_manager.get_entry(&entry.name).await.unwrap().unwrap();
        assert_eq!(stored.version.get(&peer), Some(&3));
        assert!(
            !stored.version.contains_key(&third),
            "foreign axis must not be persisted from Transfer"
        );
    }

    /// Issue #33 B1: an unsolicited Transfer (no matching outstanding
    /// Request) must be dropped at the application layer before any
    /// DB write — even if the bytes already made it to staging. The
    /// poisoned-counter rejection path lives at the TCP layer (see
    /// `tcp::receiver`); here we cover the orthogonal app-layer guard.
    #[tokio::test]
    async fn handle_transfer_drops_unsolicited_transfer() {
        let (env, receiver, entry_manager, mut send_rx) = setup().await;
        let mut sse_rx = env.state.sse_subscribe();
        let peer = Uuid::new_v4();
        let entry = file_entry("sync/payload.bin");
        let staging = make_staged_transfer(&env, b"unsolicited").await;
        let staging_root = staging.path().unwrap().parent().unwrap().to_path_buf();

        let evt = TransportEvent {
            payload: TransportData::Transfer(entry.clone()),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: Some(staging),
        };
        receiver.handle_transfer(evt).await.unwrap();

        assert!(
            entry_manager
                .get_entry(&entry.name)
                .await
                .unwrap()
                .is_none(),
            "unsolicited Transfer must not be persisted"
        );
        assert!(
            !env.home_path().join(&*entry.name).exists(),
            "unsolicited Transfer must not write to home"
        );
        assert!(
            !staging_root.exists(),
            "staging dir must be cleaned up on drop"
        );
        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
        // The GUI must see a sync_failed so an earlier `EntrySyncStarted`
        // does not stay pending forever.
        match sse_rx.try_recv().expect("expected EntrySyncFailed") {
            ServerEvent::EntrySyncFailed { reason, .. } => {
                assert!(reason.contains("unsolicited"), "reason was: {reason}");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    /// Issue #33 B1: when we requested a transfer but locally edited
    /// the entry since (so the local row strictly dominates the peer's
    /// vector), the commit path must drop the staged bytes rather than
    /// overwrite the newer local edit. The pre-fix flow would have
    /// already renamed the staged bytes into `home_path` from the TCP
    /// layer before this comparison ever ran.
    #[tokio::test]
    async fn handle_transfer_drops_when_local_now_newer() {
        let (env, receiver, entry_manager, _send_rx) = setup().await;
        let mut sse_rx = env.state.sse_subscribe();
        let local_id = env.state.local_id();
        let peer = Uuid::new_v4();
        let name: RelativePath = "sync/payload.bin".into();

        // We requested the Transfer when peer was at peer-axis=1.
        // Before the Transfer arrived, we locally edited on top (local
        // axis bumped to 5). Our row strictly dominates the peer's
        // sanitized view ({peer: 1}) on every axis → KeepSelf,
        // commit_staged_transfer must drop the bytes.
        entry_manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("local-newer".into()),
                version: HashMap::from([(local_id, 5), (peer, 1)]),
            })
            .await
            .unwrap();
        let home_file = env.home_path().join(&*name);
        tokio::fs::create_dir_all(home_file.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&home_file, b"local-newer").await.unwrap();

        let peer_entry = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("stale-peer".into()),
            version: HashMap::from([(peer, 1)]),
        };
        env.state.register_pending_request(peer, name.clone()).await;
        let staging = make_staged_transfer(&env, b"stale-peer").await;
        let staging_root = staging.path().unwrap().parent().unwrap().to_path_buf();

        let evt = TransportEvent {
            payload: TransportData::Transfer(peer_entry),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: Some(staging),
        };
        receiver.handle_transfer(evt).await.unwrap();

        // Home file is unchanged.
        assert_eq!(
            tokio::fs::read(&home_file).await.unwrap(),
            b"local-newer",
            "local edit must not be overwritten by stale Transfer"
        );
        // Staging cleaned up.
        assert!(!staging_root.exists());
        // GUI sees the drop as a sync failure.
        match sse_rx.try_recv().expect("expected EntrySyncFailed") {
            ServerEvent::EntrySyncFailed { reason, .. } => {
                assert!(reason.contains("local newer"), "reason was: {reason}");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    /// Issue #33 B1: a successful commit renames the staged bytes
    /// atomically into `home_path` and persists the sanitized peer
    /// metadata.
    #[tokio::test]
    async fn handle_transfer_commits_staged_bytes_on_keep_other() {
        let (env, receiver, entry_manager, _send_rx) = setup().await;
        let peer = Uuid::new_v4();
        let entry = file_entry("sync/payload.bin");
        let contents = b"peer-bytes-committed";

        env.state
            .register_pending_request(peer, entry.name.clone())
            .await;
        let staging = make_staged_transfer(&env, contents).await;
        let staging_root = staging.path().unwrap().parent().unwrap().to_path_buf();

        let evt = TransportEvent {
            payload: TransportData::Transfer(entry.clone()),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: Some(staging),
        };
        receiver.handle_transfer(evt).await.unwrap();

        let on_disk = tokio::fs::read(env.home_path().join(&*entry.name))
            .await
            .unwrap();
        assert_eq!(on_disk, contents);
        assert!(
            entry_manager
                .get_entry(&entry.name)
                .await
                .unwrap()
                .is_some(),
            "committed entry must be persisted"
        );
        assert!(!staging_root.exists(), "staging dir cleaned up on commit");
    }

    /// Issue #33 B1: `register_pending_request` followed by
    /// `take_pending_request` is the legitimate cycle; a second
    /// `take_pending_request` with the same key must return false so
    /// a replayed Transfer cannot resurrect a consumed registration.
    #[tokio::test]
    async fn pending_request_is_consumed_on_take() {
        let env = crate::utils::test_support::test_env().await;
        let peer = Uuid::new_v4();
        let name: RelativePath = "sync/payload.bin".into();
        env.state.register_pending_request(peer, name.clone()).await;
        assert!(env.state.take_pending_request(peer, &name).await);
        assert!(!env.state.take_pending_request(peer, &name).await);
    }

    #[tokio::test]
    async fn handle_transfer_drops_entries_outside_configured_sync_dirs() {
        let (env, receiver, entry_manager, mut send_rx) = setup().await;
        let mut sse_rx = env.state.sse_subscribe();
        let entry = file_entry("other/payload.bin");

        receiver
            .handle_transfer(event(TransportData::Transfer(entry.clone())))
            .await
            .unwrap();

        assert!(
            entry_manager
                .get_entry(&entry.name)
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(send_rx.try_recv(), Err(TryRecvError::Empty)));
        assert!(sse_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_metadata_emits_sync_started_when_keep_other_file() {
        let (env, receiver, _entry_manager, _send_rx) = setup().await;
        let mut sse_rx = env.state.sse_subscribe();
        let peer = Uuid::new_v4();
        // Peer entry has a version on its own device id — nothing locally,
        // so handle_metadata yields KeepOther and enqueues a Request.
        let entry = EntryInfo {
            name: "sync/payload.bin".into(),
            kind: EntryKind::File,
            hash: Some("hash".to_string()),
            version: HashMap::from([(peer, 1)]),
        };

        let evt = TransportEvent {
            payload: TransportData::Metadata(entry.clone()),
            metadata: TransportMetadata {
                source_id: peer,
                source_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            staging: None,
        };
        receiver.handle_metadata(evt).await.unwrap();

        match sse_rx.try_recv().expect("expected EntrySyncStarted") {
            ServerEvent::EntrySyncStarted {
                dir,
                relative_path,
                peer: emitted_peer,
            } => {
                assert_eq!(dir.as_ref() as &str, "sync");
                assert_eq!(relative_path, entry.name);
                assert_eq!(emitted_peer, peer);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
