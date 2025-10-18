use crate::{
    application::{
        AppState, EntryManager, PeerManager,
        network::transport::interface::{
            HandshakeData, HandshakeKind, TransportDataV2, TransportInterfaceV2,
            TransportRecvEvent, TransportSendEvent,
        },
        persistence::interface::PersistenceInterface,
    },
    domain::{EntryInfo, Peer},
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{
    fs, io,
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
};
use tracing::{error, info, warn};

pub struct TransportService<T: TransportInterfaceV2, P: PersistenceInterface> {
    adapter: T,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_chan: TransportChannel<TransportSendEvent>,
    data_chan: TransportChannel<TransportRecvEvent>,
    control_chan: TransportChannel<TransportRecvEvent>,
}

impl<T: TransportInterfaceV2, P: PersistenceInterface> TransportService<T, P> {
    pub fn new(
        adapter: T,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
    ) -> (Sender<TransportSendEvent>, Self) {
        let send_chan = TransportChannel::new();

        (
            send_chan.tx.clone(),
            Self {
                state,
                adapter,
                peer_manager,
                entry_manager,
                send_chan,
                data_chan: TransportChannel::new(),
                control_chan: TransportChannel::new(),
            },
        )
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(
            self.send(),
            self.recv(),
            self.recv_data(),
            self.recv_control()
        )?;
        Ok(())
    }

    async fn send(&self) -> io::Result<()> {
        while let Some(event) = self.send_chan.rx.lock().await.recv().await {
            let _ = self.adapter.send(event).await;
        }
        Ok(())
    }

    async fn recv(&self) -> io::Result<()> {
        loop {
            let event = self.adapter.recv().await?;
            match event.data {
                TransportDataV2::Transfer(_) => {
                    let _ = self.data_chan.tx.send(event).await;
                }

                _ => {
                    let _ = self.control_chan.tx.send(event).await;
                }
            }
        }
    }

    async fn recv_data(&self) -> io::Result<()> {
        while let Some(event) = self.data_chan.rx.lock().await.recv().await {
            self.handle_transfer(event).await?;
        }
        Ok(())
    }

    async fn recv_control(&self) -> io::Result<()> {
        while let Some(event) = self.control_chan.rx.lock().await.recv().await {
            match event.data {
                TransportDataV2::Handshake(_) => {
                    self.handle_handshake(event).await?;
                }

                TransportDataV2::Metadata(_) => {
                    self.handle_metadata(event).await?;
                }

                TransportDataV2::Request(_) => {
                    self.handle_request(event).await?;
                }

                _ => unreachable!(),
            }
        }
        Ok(())
    }

    async fn handle_handshake(&self, event: TransportRecvEvent) -> io::Result<()> {
        let (hs_data, kind) = match event.data {
            TransportDataV2::Handshake(data) => data,
            _ => unreachable!(),
        };

        let peer = Peer::new(event.src_id, event.src_ip, Some(hs_data.sync_dirs));
        self.peer_manager.insert(peer.clone());

        if matches!(kind, HandshakeKind::Request) {
            // Can't use send_tx because Response must be sent strictly BEFORE syncing
            let data = self.entry_manager.get_handshake_data().await;
            self.try_send(
                || {
                    self.adapter
                        .send(TransportSendEvent {
                            target: peer.addr,
                            data: TransportDataV2::Handshake((
                                HandshakeData {
                                    sync_dirs: data.sync_directories.clone(),
                                    entries: data.entries.clone(),
                                },
                                HandshakeKind::Response,
                            )),
                        })
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
                self.send_chan
                    .tx
                    .send(TransportSendEvent {
                        target: peer.addr,
                        data: TransportDataV2::Request(entry),
                    })
                    .await
                    .map_err(io::Error::other)?;
            } else {
                // self.create_received_dir(entry).await?;
            }
        }
        Ok(())
    }

    async fn handle_metadata(&self, event: TransportRecvEvent) -> io::Result<()> {
        Ok(())
    }

    async fn handle_request(&self, event: TransportRecvEvent) -> io::Result<()> {
        Ok(())
    }

    async fn handle_transfer(&self, event: TransportRecvEvent) -> io::Result<()> {
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
