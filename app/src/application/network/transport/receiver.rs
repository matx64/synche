use crate::{
    application::{
        EntryManager, PeerManager,
        network::{
            TransportInterface,
            transport::interface::{ReceiverChannel, TransportData, TransportSenders},
        },
        persistence::interface::PersistenceInterface,
    },
    domain::{CanonicalPath, EntryInfo, Peer, VersionCmp},
    proto::transport::{SyncEntryKind, SyncHandshakeKind, SyncKind},
};
use std::{env, net::IpAddr, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{self, AsyncWriteExt},
};
use tracing::{error, info, warn};

pub struct TransportReceiver<T: TransportInterface, P: PersistenceInterface> {
    transport_adapter: Arc<T>,
    entry_manager: Arc<EntryManager<P>>,
    peer_manager: Arc<PeerManager>,
    senders: TransportSenders,
    control_chan: ReceiverChannel<T>,
    data_chan: ReceiverChannel<T>,
    base_dir_path: CanonicalPath,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportReceiver<T, P> {
    pub fn new(
        transport_adapter: Arc<T>,
        entry_manager: Arc<EntryManager<P>>,
        peer_manager: Arc<PeerManager>,
        senders: TransportSenders,
        base_dir_path: CanonicalPath,
    ) -> Self {
        Self {
            control_chan: ReceiverChannel::new(),
            data_chan: ReceiverChannel::new(),
            transport_adapter,
            entry_manager,
            peer_manager,
            senders,
            base_dir_path,
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.recv(), self.recv_control(), self.recv_data())?;
        Ok(())
    }

    pub async fn recv(&self) -> io::Result<()> {
        loop {
            let data = self.transport_adapter.recv().await?;

            match data.kind {
                SyncKind::Entry(SyncEntryKind::Transfer) => {
                    let _ = self.data_chan.tx.send(data).await;
                }

                _ => {
                    let _ = self.control_chan.tx.send(data).await;
                }
            }
        }
    }

    pub async fn recv_data(&self) -> io::Result<()> {
        loop {
            if let Some(data) = self.data_chan.rx.lock().await.recv().await {
                self.handle_transfer(data).await?;
            }
        }
    }

    pub async fn recv_control(&self) -> io::Result<()> {
        loop {
            if let Some(data) = self.control_chan.rx.lock().await.recv().await {
                match data.kind {
                    SyncKind::Handshake(_) => self.handle_handshake(data).await?,
                    SyncKind::Entry(SyncEntryKind::Metadata) => self.handle_metadata(data).await?,
                    SyncKind::Entry(SyncEntryKind::Request) => self.handle_request(data).await?,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub async fn handle_handshake(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let peer_hs_data = self
            .transport_adapter
            .read_handshake(&mut data.stream)
            .await?;

        let peer = Peer::new(
            data.src_id,
            data.src_ip,
            Some(peer_hs_data.sync_directories),
        );
        self.peer_manager.insert(peer.clone());

        if matches!(data.kind, SyncKind::Handshake(SyncHandshakeKind::Request)) {
            // Can't use handshake_tx because Response must be sent strictly BEFORE syncing
            let data = self.entry_manager.get_handshake_data().await;
            self.try_send(
                || {
                    self.transport_adapter.send_handshake(
                        peer.addr,
                        SyncKind::Handshake(SyncHandshakeKind::Response),
                        data.clone(),
                    )
                },
                peer.addr,
            )
            .await;
        }

        info!(peer = ?peer.id, "🔁  Syncing Peer...");

        let entries_to_request = self
            .entry_manager
            .get_entries_to_request(&peer, peer_hs_data.entries)
            .await?;

        for entry in entries_to_request {
            if entry.is_file() {
                self.senders
                    .request_tx
                    .send((peer.addr, entry))
                    .await
                    .map_err(io::Error::other)?;
            } else {
                self.create_received_dir(entry).await?;
            }
        }
        Ok(())
    }

    pub async fn handle_metadata(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let peer_entry = self
            .transport_adapter
            .read_metadata(&mut data.stream)
            .await?;

        match self
            .entry_manager
            .handle_metadata(data.src_id, &peer_entry)
            .await?
        {
            VersionCmp::KeepOther => {
                if peer_entry.is_removed() {
                    self.remove_entry(&peer_entry.name).await
                } else if peer_entry.is_file() {
                    self.senders
                        .request_tx
                        .send((data.src_ip, peer_entry))
                        .await
                        .map_err(io::Error::other)
                } else {
                    self.create_received_dir(peer_entry).await
                }
            }

            _ => Ok(()),
        }
    }

    pub async fn handle_request(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let requested_entry = self
            .transport_adapter
            .read_request(&mut data.stream)
            .await?;

        match self.entry_manager.get_entry(&requested_entry.name).await {
            Some(local_entry)
                if local_entry.is_file()
                    && matches!(local_entry.compare(&requested_entry), VersionCmp::Equal) =>
            {
                self.senders
                    .transfer_tx
                    .send((data.src_ip, local_entry))
                    .await
                    .map_err(io::Error::other)
            }

            _ => Ok(()),
        }
    }

    pub async fn handle_transfer(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let (entry, contents) = self.transport_adapter.read_entry(&mut data.stream).await?;

        let entry = self.entry_manager.insert_entry(entry).await;

        let original_path = self.base_dir_path.join(&*entry.name);
        let tmp_path = env::temp_dir().join(&*entry.name);

        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(&contents).await?;
        tmp_file.flush().await?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await?;

        self.senders
            .metadata_tx
            .send(entry)
            .await
            .map_err(io::Error::other)
    }

    pub async fn create_received_dir(&self, dir: EntryInfo) -> io::Result<()> {
        let dir = self.entry_manager.insert_entry(dir).await;

        let path = self.base_dir_path.join(&*dir.name);
        fs::create_dir_all(path).await?;

        self.senders
            .metadata_tx
            .send(dir)
            .await
            .map_err(io::Error::other)
    }

    pub async fn remove_entry(&self, entry_name: &str) -> io::Result<()> {
        let _ = self.entry_manager.remove_entry(entry_name).await;

        let path = self.base_dir_path.join(entry_name);

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

            if !self.peer_manager.exists(addr) {
                warn!("⚠️  Cancelled transport send op because peer disconnected during process.");
                return;
            }
        }

        error!(peer = ?addr, "Disconnecting peer after 3 Transport send attempts.");
        self.peer_manager.remove_peer_by_addr(addr);
    }
}
