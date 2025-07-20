use crate::{
    application::network::{
        TransportInterface,
        transport::interface::{
            TransportInfo, TransportReceivers, TransportSenders, TransportStreamExt,
        },
    },
    domain::{FileInfo, Peer, PeerManager},
    proto::tcp::{SyncFileKind, SyncHandshakeKind, SyncKind},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    io,
    sync::mpsc::{self, Receiver},
    time,
};
use tracing::info;

pub struct TransportSender<T: TransportInterface> {
    transport_adapter: T,
    peer_manager: Arc<PeerManager>,
    senders: TransportSenders,
    receivers: TransportReceivers,
}

impl<T: TransportInterface> TransportSender<T> {
    pub fn new(transport_adapter: T, peer_manager: Arc<PeerManager>) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel::<FileInfo>(100);
        let (transfer_tx, transfer_rx) = mpsc::channel::<TransportInfo>(100);
        let (control_tx, control_rx) = mpsc::channel::<(SyncKind, TransportInfo)>(100);

        Self {
            transport_adapter,
            peer_manager,
            senders: TransportSenders {
                watch_tx,
                transfer_tx,
                control_tx,
            },
            receivers: TransportReceivers {
                watch_rx,
                transfer_rx,
                control_rx,
            },
        }
    }

    pub async fn send_file_changes(&mut self) -> io::Result<()> {
        let mut buffer = HashMap::<String, FileInfo>::new();
        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                Some(file) = self.receivers.watch_rx.recv() => {
                    info!("File added to buffer: {}", file.name);
                    buffer.insert(file.name.clone(), file);
                },

                _ = interval.tick() => {
                    if buffer.is_empty() {
                        continue;
                    }

                    info!("Synching files: {:?}", buffer);

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

    pub async fn send_control(&mut self) {
        loop {
            if let Some((kind, info)) = self.receivers.control_rx.recv().await {
                match kind {
                    SyncKind::File(SyncFileKind::Metadata) => {
                        let _ = self
                            .transport_adapter
                            .send_metadata(info.target_addr, &info.file_info)
                            .await;
                    }
                    SyncKind::File(SyncFileKind::Request) => {
                        let _ = self
                            .transport_adapter
                            .send_request(info.target_addr, &info.file_info)
                            .await;
                    }
                    _ => {}
                }
            }
        }
    }
}
