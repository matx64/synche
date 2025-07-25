use crate::{
    application::network::{
        TransportInterface,
        transport::interface::{TransportData, TransportSenders},
    },
    domain::{EntryManager, Peer, PeerManager, entry::VersionVectorCmp},
    proto::transport::{SyncFileKind, SyncHandshakeKind, SyncKind},
};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{self, AsyncWriteExt},
};
use tracing::{info, warn};

pub struct TransportReceiver<T: TransportInterface> {
    transport_adapter: Arc<T>,
    entry_manager: Arc<EntryManager>,
    peer_manager: Arc<PeerManager>,
    senders: TransportSenders,
    base_dir: PathBuf,
    tmp_dir: PathBuf,
}

impl<T: TransportInterface> TransportReceiver<T> {
    pub fn new(
        transport_adapter: Arc<T>,
        entry_manager: Arc<EntryManager>,
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

    pub async fn recv(&self) -> io::Result<()> {
        loop {
            let data = self.transport_adapter.recv().await?;

            // TODO: Async
            match data.kind {
                SyncKind::Handshake(_) => {
                    self.handle_handshake(data).await?;
                }
                SyncKind::File(SyncFileKind::Metadata) => {
                    self.handle_metadata(data).await?;
                }
                SyncKind::File(SyncFileKind::Request) => {
                    self.handle_request(data).await?;
                }
                SyncKind::File(SyncFileKind::Transfer) => {
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

        let peer = Peer::new(data.src_id, data.src_addr, Some(sync_data.directories));
        self.peer_manager.insert(peer.clone());

        if matches!(data.kind, SyncKind::Handshake(SyncHandshakeKind::Request)) {
            self.senders
                .handshake_tx
                .send((data.src_addr, SyncHandshakeKind::Response))
                .await
                .map_err(io::Error::other)?;
        }

        info!("Synching peer: {}", data.src_addr.ip());

        let files_to_send = self.entry_manager.get_files_to_send(&peer, sync_data.files);

        for file in files_to_send {
            self.senders
                .transfer_tx
                .send((data.src_addr, file))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    pub async fn handle_metadata(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let peer_file = self
            .transport_adapter
            .read_metadata(&mut data.stream)
            .await?;

        let is_deleted = peer_file.is_deleted();

        if is_deleted && self.entry_manager.get_file(&peer_file.name).is_none() {
            return Ok(());
        }

        let cmp = self.entry_manager.handle_metadata(data.src_id, &peer_file);
        match cmp {
            VersionVectorCmp::KeepPeer => {
                if is_deleted {
                    self.remove_file(&peer_file.name).await
                } else {
                    self.senders
                        .request_tx
                        .send((data.src_addr, peer_file))
                        .await
                        .map_err(io::Error::other)
                }
            }
            VersionVectorCmp::Conflict => {
                // TODO: Conflict
                warn!("Metadata Conflict in file: {}", peer_file.name);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub async fn handle_request(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let requested_file = self
            .transport_adapter
            .read_request(&mut data.stream)
            .await?;

        if let Some(file) = self.entry_manager.get_file(&requested_file.name) {
            if file.hash == requested_file.hash {
                self.senders
                    .transfer_tx
                    .send((data.src_addr, requested_file))
                    .await
                    .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }

    pub async fn handle_transfer(&self, mut data: TransportData<T::Stream>) -> io::Result<()> {
        let (file, contents) = self.transport_adapter.read_file(&mut data.stream).await?;

        self.entry_manager.insert_file(file.clone());

        let original_path = self.base_dir.join(&file.name);
        let tmp_path = self.tmp_dir.join(&file.name);

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
            .send(file)
            .await
            .map_err(io::Error::other)
    }

    pub async fn remove_file(&self, file_name: &str) -> io::Result<()> {
        let _ = self.entry_manager.remove_file(file_name);

        let path = self.base_dir.join(file_name);
        fs::remove_file(path).await
    }
}
