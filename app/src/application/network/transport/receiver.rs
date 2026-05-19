use crate::{
    application::{
        AppState, EntryManager, PeerManager, network::transport::interface::TransportInterface,
        persistence::interface::PersistenceInterface,
    },
    domain::{
        EntryInfo, MutexChannel, Peer, TransportChannelData, TransportData, TransportEvent,
        VersionCmp,
    },
    utils::fs::is_git_path,
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::mpsc::Sender};
use tracing::{error, info, warn};

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

        info!(peer = ?peer.id, "🔁  Syncing Peer...");

        let entries_to_request = self
            .entry_manager
            .get_entries_to_request(&peer, hs_data.entries)
            .await?;

        for entry in entries_to_request {
            if entry.is_file() {
                self.send_tx
                    .send(TransportChannelData::Request((peer.addr, entry)))
                    .await
                    .map_err(io::Error::other)?;
            } else {
                self.create_received_dir(entry).await?;
            }
        }
        Ok(())
    }

    async fn handle_metadata(&self, event: TransportEvent) -> io::Result<()> {
        let peer_entry = match event.payload {
            TransportData::Metadata(entry) => entry,
            _ => unreachable!(),
        };

        if is_git_path(&peer_entry.name) {
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
                    self.send_tx
                        .send(TransportChannelData::Request((
                            event.metadata.source_ip,
                            peer_entry,
                        )))
                        .await
                        .map_err(io::Error::other)
                } else {
                    self.create_received_dir(peer_entry).await
                }
            }

            _ => Ok(()),
        }
    }

    async fn handle_request(&self, event: TransportEvent) -> io::Result<()> {
        let requested_entry = match event.payload {
            TransportData::Request(entry) => entry,
            _ => unreachable!(),
        };

        if is_git_path(&requested_entry.name) {
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

    async fn handle_transfer(&self, event: TransportEvent) -> io::Result<()> {
        let received_entry = match event.payload {
            TransportData::Transfer(entry) => entry,
            _ => unreachable!(),
        };

        if is_git_path(&received_entry.name) {
            return Ok(());
        }

        let entry = self.entry_manager.insert_entry(received_entry).await?;

        self.send_tx
            .send(TransportChannelData::Metadata(entry))
            .await
            .map_err(io::Error::other)
    }

    async fn create_received_dir(&self, dir: EntryInfo) -> io::Result<()> {
        let dir = self.entry_manager.insert_entry(dir).await?;

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
                warn!("⚠️  Cancelled transport send op because peer disconnected during process.");
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
        application::network::transport::interface::TransportResult,
        domain::{EntryKind, TransportMetadata},
        infra::persistence::sqlite::SqliteDb,
    };
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
    };
    use tokio::sync::mpsc::error::TryRecvError;
    use uuid::Uuid;

    struct MockTransport;

    impl TransportInterface for MockTransport {
        async fn recv(&self) -> TransportResult<TransportEvent> {
            Err(
                crate::application::network::transport::interface::TransportError::new(
                    "unused mock recv",
                ),
            )
        }

        async fn send(&self, _target: IpAddr, _data: TransportData) -> TransportResult<()> {
            Ok(())
        }
    }

    async fn setup() -> (
        TransportReceiver<MockTransport, SqliteDb>,
        Arc<EntryManager<SqliteDb>>,
        tokio::sync::mpsc::Receiver<TransportChannelData>,
    ) {
        let state = AppState::new().await;
        let db = SqliteDb::new(":memory:").await.unwrap();
        let entry_manager = EntryManager::new(db, state.clone());
        let peer_manager = PeerManager::new(state.clone());
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(4);
        let receiver = TransportReceiver::new(
            Arc::new(MockTransport),
            state,
            peer_manager,
            entry_manager.clone(),
            send_tx,
        );

        (receiver, entry_manager, send_rx)
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
        }
    }

    #[tokio::test]
    async fn handle_request_ignores_git_entries_without_enqueuing_transfer() {
        let (receiver, entry_manager, mut send_rx) = setup().await;
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
        let (receiver, entry_manager, mut send_rx) = setup().await;
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
}
