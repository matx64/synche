use crate::{
    application::{
        EntryManager, PeerManager,
        network::{
            TransportInterface,
            transport::interface::{TransportReceivers, TransportSenders},
        },
        persistence::interface::PersistenceInterface,
    },
    domain::FileInfo,
    proto::transport::{SyncHandshakeKind, SyncKind},
};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
    sync::{
        Mutex,
        mpsc::{self},
    },
    time,
};
use tracing::info;

pub struct TransportSender<T: TransportInterface, D: PersistenceInterface> {
    transport_adapter: Arc<T>,
    entry_manager: Arc<EntryManager<D>>,
    peer_manager: Arc<PeerManager>,
    receivers: TransportReceivers,
    base_dir: PathBuf,
}

impl<T: TransportInterface, D: PersistenceInterface> TransportSender<T, D> {
    pub fn new(
        transport_adapter: Arc<T>,
        entry_manager: Arc<EntryManager<D>>,
        peer_manager: Arc<PeerManager>,
        base_dir: PathBuf,
    ) -> (Self, TransportSenders) {
        let (watch_tx, watch_rx) = mpsc::channel::<FileInfo>(100);
        let (handshake_tx, handshake_rx) = mpsc::channel::<(SocketAddr, SyncHandshakeKind)>(100);
        let (request_tx, request_rx) = mpsc::channel::<(SocketAddr, FileInfo)>(100);
        let (transfer_tx, transfer_rx) = mpsc::channel::<(SocketAddr, FileInfo)>(100);

        (
            Self {
                transport_adapter,
                entry_manager,
                peer_manager,
                base_dir,
                receivers: TransportReceivers {
                    watch_rx: Mutex::new(watch_rx),
                    handshake_rx: Mutex::new(handshake_rx),
                    request_rx: Mutex::new(request_rx),
                    transfer_rx: Mutex::new(transfer_rx),
                },
            },
            TransportSenders {
                watch_tx,
                handshake_tx,
                request_tx,
                transfer_tx,
            },
        )
    }

    pub async fn send_file_changes(&self) -> io::Result<()> {
        let mut buffer = HashMap::<String, FileInfo>::new();
        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                Some(file) = async {
                    let mut watch_rx = self.receivers.watch_rx.lock().await;
                    watch_rx.recv().await
                } => {
                    info!("🗃️  Adding changed file to buffer: {}", file.name);
                    buffer.insert(file.name.clone(), file);
                },

                _ = interval.tick() => {
                    if buffer.is_empty() {
                        continue;
                    }

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

    pub async fn send_handshakes(&self) -> io::Result<()> {
        loop {
            if let Some((addr, kind)) = self.receivers.handshake_rx.lock().await.recv().await {
                let data = self.entry_manager.get_sync_data();
                self.transport_adapter
                    .send_handshake(addr, SyncKind::Handshake(kind), data)
                    .await?;
            }
        }
    }

    pub async fn send_requests(&self) -> io::Result<()> {
        loop {
            if let Some((addr, file)) = self.receivers.request_rx.lock().await.recv().await {
                self.transport_adapter.send_request(addr, &file).await?;
            }
        }
    }

    pub async fn send_files(&self) -> io::Result<()> {
        loop {
            if let Some((addr, file)) = self.receivers.transfer_rx.lock().await.recv().await {
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
