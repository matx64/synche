use crate::{
    application::{
        EntryManager, PeerManager, network::transport::interface::TransportInterface,
        persistence::interface::PersistenceInterface,
    },
    domain::{AppState, Channel, EntryInfo, TransportChannelData, TransportData},
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{
    io,
    sync::{Mutex, mpsc::Receiver},
};
use tracing::{error, warn};

pub struct TransportSender<T: TransportInterface, P: PersistenceInterface> {
    adapter: Arc<T>,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_rx: Mutex<Receiver<TransportChannelData>>,
    control_chan: Channel<TransportChannelData>,
    transfer_chan: Channel<(IpAddr, EntryInfo)>,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportSender<T, P> {
    pub fn new(
        adapter: Arc<T>,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        send_rx: Mutex<Receiver<TransportChannelData>>,
    ) -> Self {
        Self {
            state,
            adapter,
            peer_manager,
            entry_manager,
            send_rx,
            control_chan: Channel::new(100),
            transfer_chan: Channel::new(16),
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.send(), self.send_control(), self.send_files())?;
        Ok(())
    }

    async fn send(&self) -> io::Result<()> {
        while let Some(data) = self.send_rx.lock().await.recv().await {
            match data {
                TransportChannelData::Transfer(data) => {
                    self.transfer_chan
                        .tx
                        .send(data)
                        .await
                        .map_err(io::Error::other)?;
                }

                _ => {
                    self.control_chan
                        .tx
                        .send(data)
                        .await
                        .map_err(io::Error::other)?;
                }
            }
        }
        Ok(())
    }

    async fn send_control(&self) -> io::Result<()> {
        while let Some(data) = self.control_chan.rx.lock().await.recv().await {
            match data {
                TransportChannelData::HandshakeSyn(target) => {
                    self.send_handshake(target, true).await?;
                }

                TransportChannelData::_HandshakeAck(target) => {
                    self.send_handshake(target, false).await?;
                }

                TransportChannelData::Metadata(entry) => {
                    self.send_metadata(entry).await?;
                }

                TransportChannelData::Request((target, entry)) => {
                    self.send_request(target, entry).await?;
                }

                _ => unreachable!(),
            }
        }
        Ok(())
    }

    async fn send_handshake(&self, target: IpAddr, is_syn: bool) -> io::Result<()> {
        let data = self.entry_manager.get_handshake_data().await;

        self.try_send(
            || {
                let data = if is_syn {
                    TransportData::HandshakeSyn(data.clone())
                } else {
                    TransportData::HandshakeAck(data.clone())
                };

                self.adapter.send(target, data).map_err(|e| e.into())
            },
            target,
        )
        .await;

        Ok(())
    }

    async fn send_metadata(&self, entry: EntryInfo) -> io::Result<()> {
        for target in self.peer_manager.get_peers_to_send_metadata(&entry).await {
            self.try_send(
                || {
                    self.adapter
                        .send(target, TransportData::Metadata(entry.clone()))
                        .map_err(|e| e.into())
                },
                target,
            )
            .await;
        }
        Ok(())
    }

    async fn send_request(&self, target: IpAddr, entry: EntryInfo) -> io::Result<()> {
        self.try_send(
            || {
                self.adapter
                    .send(target, TransportData::Request(entry.clone()))
                    .map_err(|e| e.into())
            },
            target,
        )
        .await;
        Ok(())
    }

    async fn send_files(&self) -> io::Result<()> {
        while let Some((target, entry)) = self.transfer_chan.rx.lock().await.recv().await {
            let path = self.state.home_path.join(&*entry.name);

            if !path.exists() || !path.is_file() {
                continue;
            }

            self.try_send(
                || {
                    self.adapter
                        .send(target, TransportData::Transfer(entry.clone()))
                        .map_err(|e| e.into())
                },
                target,
            )
            .await;
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
