use crate::{
    application::network::{
        TransportInterface,
        transport::interface::{TransportSenders, TransportStreamExt},
    },
    domain::{EntryManager, Peer, PeerManager},
    proto::tcp::{SyncFileKind, SyncHandshakeKind, SyncKind},
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
            let (stream, kind) = self.transport_adapter.recv().await?;

            // TODO: Async
            match kind {
                SyncKind::Handshake(kind) => {
                    self.handle_handshake(stream, kind).await?;
                }
                SyncKind::File(SyncFileKind::Metadata) => {
                    self.handle_metadata(stream).await?;
                }
                SyncKind::File(SyncFileKind::Request) => {
                    self.handle_request(stream).await?;
                }
                SyncKind::File(SyncFileKind::Transfer) => {
                    self.handle_transfer(stream).await?;
                }
            }
        }
    }

    pub async fn handle_handshake(
        &self,
        mut stream: T::Stream,
        kind: SyncHandshakeKind,
    ) -> io::Result<()> {
        let src_addr = stream.peer_addr()?;
        let src_ip = src_addr.ip();

        let data = self.transport_adapter.read_handshake(&mut stream).await?;

        let peer = Peer::new(src_addr, Some(data));
        self.peer_manager.insert(peer.clone());

        if matches!(kind, SyncHandshakeKind::Request) {
            self.senders
                .handshake_tx
                .send((src_addr, SyncHandshakeKind::Response))
                .await
                .map_err(io::Error::other)?;
        }

        info!("Synching peer: {}", src_ip);

        let files_to_send = self.entry_manager.get_files_to_send(&peer);

        for file in files_to_send {
            self.senders
                .transfer_tx
                .send((src_addr, file))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    pub async fn handle_metadata(&self, mut stream: T::Stream) -> io::Result<()> {
        let src_addr = stream.peer_addr()?;
        let src_ip = src_addr.ip();

        let peer_file = self.transport_adapter.read_metadata(&mut stream).await?;

        let is_deleted = peer_file.is_deleted();
        if is_deleted {
            self.peer_manager.remove_file(&src_ip, &peer_file.name);
        } else {
            self.peer_manager.insert_file(&src_ip, peer_file.clone());
        }

        match self.entry_manager.get_file(&peer_file.name) {
            Some(local_file) => {
                if local_file.hash != peer_file.hash {
                    if local_file.version < peer_file.version {
                        if is_deleted {
                            self.remove_file(&peer_file.name).await?;
                        } else {
                            self.transport_adapter
                                .send_request(src_addr, &peer_file)
                                .await?;
                        }
                    } else if local_file.version == peer_file.version {
                        // TODO: Handle Conflict
                        warn!("FILE VERSION CONFLICT: {}", local_file.name);
                    }
                }
            }

            None => {
                if !is_deleted {
                    self.transport_adapter
                        .send_request(src_addr, &peer_file)
                        .await?;
                }
            }
        }
        Ok(())
    }

    pub async fn handle_request(&self, mut stream: T::Stream) -> io::Result<()> {
        let requested_file = self.transport_adapter.read_request(&mut stream).await?;

        if let Some(file) = self.entry_manager.get_file(&requested_file.name) {
            if file.hash == requested_file.hash && file.version == requested_file.version {
                self.senders
                    .transfer_tx
                    .send((stream.peer_addr()?, requested_file))
                    .await
                    .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }

    pub async fn handle_transfer(&self, mut stream: T::Stream) -> io::Result<()> {
        let (file, contents) = self.transport_adapter.read_file(&mut stream).await?;

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
        let removed = self.entry_manager.remove_file(file_name);

        let path = self.base_dir.join(file_name);
        let _ = fs::remove_file(path).await;

        self.senders
            .watch_tx
            .send(removed)
            .await
            .map_err(io::Error::other)
    }
}
