use crate::{
    application::{
        EntryManager, PeerManager,
        network::{
            TransportInterface,
            transport::interface::{TransportData, TransportSenders},
        },
        persistence::interface::PersistenceInterface,
    },
    domain::{Peer, entry::VersionVectorCmp},
    proto::transport::{SyncEntryKind, SyncHandshakeKind, SyncKind},
};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{self, AsyncWriteExt},
};
use tracing::{info, warn};

pub struct TransportReceiver<T: TransportInterface, D: PersistenceInterface> {
    transport_adapter: Arc<T>,
    entry_manager: Arc<EntryManager<D>>,
    peer_manager: Arc<PeerManager>,
    senders: TransportSenders,
    base_dir: PathBuf,
    tmp_dir: PathBuf,
}

impl<T: TransportInterface, D: PersistenceInterface> TransportReceiver<T, D> {
    pub fn new(
        transport_adapter: Arc<T>,
        entry_manager: Arc<EntryManager<D>>,
        peer_manager: Arc<PeerManager>,
        senders: TransportSenders,
        base_dir: PathBuf,
        tmp_dir: PathBuf,
    ) -> Self {
        Self {
            transport_adapter,
            entry_manager,
            peer_manager,
            senders,
            base_dir,
            tmp_dir,
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        loop {
            let data = self.transport_adapter.recv().await?;

            // TODO: Async
            match data.kind {
                SyncKind::Handshake(_) => {
                    self.handle_handshake(data).await?;
                }
                SyncKind::Entry(SyncEntryKind::Metadata) => {
                    self.handle_metadata(data).await?;
                }
                SyncKind::Entry(SyncEntryKind::Request) => {
                    self.handle_request(data).await?;
                }
                SyncKind::Entry(SyncEntryKind::Transfer) => {
                    self.handle_transfer(data).await?;
                }
            }
        }
    }

    pub async fn handle_handshake(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let sync_data = self
            .transport_adapter
            .read_handshake(&mut data.stream)
            .await?;

        let peer = Peer::new(data.src_id, data.src_ip, Some(sync_data.directories));
        self.peer_manager.insert(peer.clone());

        if matches!(data.kind, SyncKind::Handshake(SyncHandshakeKind::Request)) {
            self.senders
                .handshake_tx
                .send((data.src_ip, SyncHandshakeKind::Response))
                .await
                .map_err(io::Error::other)?;
        }

        info!("Synching peer: {}", data.src_ip);

        let entries_to_send = self
            .entry_manager
            .get_entries_to_send(&peer, sync_data.entries);

        for entry in entries_to_send {
            self.senders
                .transfer_tx
                .send((data.src_ip, entry))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    pub async fn handle_metadata(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let peer_entry = self
            .transport_adapter
            .read_metadata(&mut data.stream)
            .await?;

        let is_deleted = peer_entry.is_deleted;

        if is_deleted && self.entry_manager.get_entry(&peer_entry.name).is_none() {
            return Ok(());
        }

        let cmp = self.entry_manager.handle_metadata(data.src_id, &peer_entry);
        match cmp {
            VersionVectorCmp::KeepPeer => {
                if is_deleted {
                    self.remove_entry(&peer_entry.name).await
                } else {
                    self.senders
                        .request_tx
                        .send((data.src_ip, peer_entry))
                        .await
                        .map_err(io::Error::other)
                }
            }
            VersionVectorCmp::Conflict => {
                // TODO: Conflict
                warn!("Metadata Conflict in entry: {}", peer_entry.name);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub async fn handle_request(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let requested_entry = self
            .transport_adapter
            .read_request(&mut data.stream)
            .await?;

        if let Some(entry) = self.entry_manager.get_entry(&requested_entry.name) {
            if entry.hash == requested_entry.hash {
                self.senders
                    .transfer_tx
                    .send((data.src_ip, requested_entry))
                    .await
                    .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }

    pub async fn handle_transfer(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let (entry, contents) = self.transport_adapter.read_entry(&mut data.stream).await?;

        self.entry_manager.insert_entry(&entry);

        let original_path = self.base_dir.join(&entry.name);
        let tmp_path = self.tmp_dir.join(&entry.name);

        if let Some(parent) = tmp_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(&contents).await?;
        tmp_file.flush().await?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await?;

        self.senders
            .watch_tx
            .send(entry)
            .await
            .map_err(io::Error::other)
    }

    pub async fn remove_entry(&self, entry_name: &str) -> io::Result<()> {
        let _ = self.entry_manager.remove_entry(entry_name);

        let path = self.base_dir.join(entry_name);
        fs::remove_file(path).await
    }
}
