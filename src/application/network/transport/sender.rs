use crate::{
    application::network::{
        TransportInterface,
        transport::interface::{TransportReceivers, TransportSenders},
    },
    domain::{EntryManager, FileInfo, PeerManager},
    proto::tcp::{SyncHandshakeKind, SyncKind},
};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
    sync::mpsc::{self},
    time,
};
use tracing::info;

pub struct TransportSender<T: TransportInterface> {
    transport_adapter: T,
    entry_manager: Arc<EntryManager>,
    peer_manager: Arc<PeerManager>,
    senders: TransportSenders,
    receivers: TransportReceivers,
    base_dir: PathBuf,
}

impl<T: TransportInterface> TransportSender<T> {
    pub fn new(
        transport_adapter: T,
        entry_manager: Arc<EntryManager>,
        peer_manager: Arc<PeerManager>,
        base_dir: PathBuf,
    ) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel::<FileInfo>(100);
        let (handshake_tx, handshake_rx) = mpsc::channel::<(SocketAddr, SyncHandshakeKind)>(100);
        let (request_tx, request_rx) = mpsc::channel::<(SocketAddr, FileInfo)>(100);
        let (transfer_tx, transfer_rx) = mpsc::channel::<(SocketAddr, FileInfo)>(100);

        Self {
            transport_adapter,
            entry_manager,
            peer_manager,
            base_dir,
            senders: TransportSenders {
                watch_tx,
                handshake_tx,
                request_tx,
                transfer_tx,
            },
            receivers: TransportReceivers {
                watch_rx,
                handshake_rx,
                request_rx,
                transfer_rx,
            },
        }
    }

    pub async fn send_file_changes(&mut self) -> io::Result<()> {
        let mut buffer = HashMap::<String, FileInfo>::new();
        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                Some(file) = self.receivers.watch_rx.recv() => {
                    info!("File changed: {}", file.name);
                    buffer.insert(file.name.clone(), file);
                },

                _ = interval.tick() => {
                    if buffer.is_empty() {
                        continue;
                    }

                    info!("Sending file changes: {:?}", buffer);

                    let sync_map = self.peer_manager.build_sync_map(&buffer);

                    for (addr, files) in sync_map {
                        for file in files {
                            self.transport_adapter.send_metadata(addr, file).await?;
                        }
                    }

                    buffer.clear();
                }
            }
        }
    }

    pub async fn send_handshakes(&mut self) -> io::Result<()> {
        loop {
            if let Some((addr, kind)) = self.receivers.handshake_rx.recv().await {
                let data = self.entry_manager.get_sync_data();
                self.transport_adapter
                    .send_handshake(addr, SyncKind::Handshake(kind), data)
                    .await?;
            }
        }
    }

    pub async fn send_requests(&mut self) -> io::Result<()> {
        loop {
            if let Some((addr, file)) = self.receivers.request_rx.recv().await {
                self.transport_adapter.send_request(addr, &file).await?;
            }
        }
    }

    pub async fn send_files(&mut self) -> io::Result<()> {
        loop {
            if let Some((addr, file)) = self.receivers.transfer_rx.recv().await {
                let path = self.base_dir.join(&file.name);

                if !path.exists() {
                    continue;
                }

                let mut fs_file = File::open(path).await?;
                let mut buffer = Vec::new();
                fs_file.read_to_end(&mut buffer).await?;

                self.transport_adapter
                    .send_file(addr, &file, &buffer)
                    .await?;
            }
        }
    }
}
