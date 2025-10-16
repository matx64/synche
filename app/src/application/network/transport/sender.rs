use crate::{
    application::{
        EntryManager, PeerManager,
        network::{
            TransportInterface,
            transport::interface::{TransportReceivers, TransportSenders},
        },
        persistence::interface::PersistenceInterface,
    },
    domain::{CanonicalPath, EntryInfo},
    proto::transport::{SyncHandshakeKind, SyncKind},
};
use sha2::{Digest, Sha256};
use std::{net::IpAddr, sync::Arc};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt},
    sync::{
        Mutex,
        mpsc::{self},
    },
};
use tracing::{error, warn};

pub struct TransportSender<T: TransportInterface, P: PersistenceInterface> {
    transport_adapter: Arc<T>,
    entry_manager: Arc<EntryManager<P>>,
    peer_manager: Arc<PeerManager>,
    receivers: TransportReceivers,
    base_dir_path: CanonicalPath,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportSender<T, P> {
    pub fn new(
        transport_adapter: Arc<T>,
        entry_manager: Arc<EntryManager<P>>,
        peer_manager: Arc<PeerManager>,
        base_dir_path: CanonicalPath,
    ) -> (Self, TransportSenders) {
        let (metadata_tx, metadata_rx) = mpsc::channel::<EntryInfo>(100);
        let (handshake_tx, handshake_rx) = mpsc::channel::<(IpAddr, SyncHandshakeKind)>(100);
        let (request_tx, request_rx) = mpsc::channel::<(IpAddr, EntryInfo)>(100);
        let (transfer_tx, transfer_rx) = mpsc::channel::<(IpAddr, EntryInfo)>(100);

        (
            Self {
                transport_adapter,
                entry_manager,
                peer_manager,
                base_dir_path,
                receivers: TransportReceivers {
                    metadata_rx: Mutex::new(metadata_rx),
                    handshake_rx: Mutex::new(handshake_rx),
                    request_rx: Mutex::new(request_rx),
                    transfer_rx: Mutex::new(transfer_rx),
                },
            },
            TransportSenders {
                metadata_tx,
                handshake_tx,
                request_tx,
                transfer_tx,
            },
        )
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(
            self.send_handshakes(),
            self.send_metadata(),
            self.send_requests(),
            self.send_files()
        )?;
        Ok(())
    }

    async fn send_metadata(&self) -> io::Result<()> {
        loop {
            if let Some(entry) = self.receivers.metadata_rx.lock().await.recv().await {
                for addr in self.peer_manager.get_peers_to_send_metadata(&entry) {
                    self.try_send(|| self.transport_adapter.send_metadata(addr, &entry), addr)
                        .await;
                }
            }
        }
    }

    async fn send_handshakes(&self) -> io::Result<()> {
        loop {
            if let Some((addr, kind)) = self.receivers.handshake_rx.lock().await.recv().await {
                let data = self.entry_manager.get_handshake_data().await;
                self.try_send(
                    || {
                        self.transport_adapter.send_handshake(
                            addr,
                            SyncKind::Handshake(kind.clone()),
                            data.clone(),
                        )
                    },
                    addr,
                )
                .await;
            }
        }
    }

    async fn send_requests(&self) -> io::Result<()> {
        loop {
            if let Some((addr, entry)) = self.receivers.request_rx.lock().await.recv().await {
                self.try_send(|| self.transport_adapter.send_request(addr, &entry), addr)
                    .await;
            }
        }
    }

    async fn send_files(&self) -> io::Result<()> {
        loop {
            if let Some((addr, entry)) = self.receivers.transfer_rx.lock().await.recv().await {
                let path = self.base_dir_path.join(&*entry.name);

                if !path.exists() || !path.is_file() {
                    continue;
                }

                let mut fs_file = File::open(path).await?;
                let mut buffer = Vec::new();
                fs_file.read_to_end(&mut buffer).await?;

                let hash = format!("{:x}", Sha256::digest(&buffer));
                if Some(hash) != entry.hash {
                    warn!("⚠️  Cancelled File Transfer because it was modified during process.");
                    continue;
                }

                self.try_send(
                    || self.transport_adapter.send_entry(addr, &entry, &buffer),
                    addr,
                )
                .await;
            }
        }
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
