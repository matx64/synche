use crate::{
    application::{
        EntryManager, PeerManager, network::transport::interface::TransportInterfaceV2,
        persistence::interface::PersistenceInterface,
    },
    domain::{
        EntryInfo,
        transport::{HandshakeKind, TransportChannel, TransportDataV2, TransportSendData},
    },
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::io;
use tracing::{error, warn};

pub struct TransportSenderV2<T: TransportInterfaceV2, P: PersistenceInterface> {
    adapter: T,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_chan: TransportChannel<TransportSendData>,
    control_chan: TransportChannel<TransportSendData>,
    transfer_chan: TransportChannel<(IpAddr, EntryInfo)>,
}

impl<T: TransportInterfaceV2, P: PersistenceInterface> TransportSenderV2<T, P> {
    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.send(), self.send_control(), self.send_files())?;
        Ok(())
    }

    async fn send(&self) -> io::Result<()> {
        while let Some(data) = self.send_chan.rx.lock().await.recv().await {
            match data {
                TransportSendData::Transfer(data) => {
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
                TransportSendData::Handshake((target, kind)) => {
                    self.send_handshake(target, kind).await?;
                }

                TransportSendData::Metadata(entry) => {
                    self.send_metadata(entry).await?;
                }

                TransportSendData::Request((target, entry)) => {
                    self.send_request(target, entry).await?;
                }

                _ => unreachable!(),
            }
        }
        Ok(())
    }

    async fn send_handshake(&self, target: IpAddr, kind: HandshakeKind) -> io::Result<()> {
        let data = self.entry_manager.get_handshake_data().await;

        self.try_send(
            || {
                self.adapter
                    .send(
                        target,
                        TransportDataV2::Handshake((data.clone(), kind.clone())),
                    )
                    .map_err(|e| e.into())
            },
            target,
        )
        .await;

        Ok(())
    }

    async fn send_metadata(&self, entry: EntryInfo) -> io::Result<()> {
        Ok(())
    }

    async fn send_request(&self, target: IpAddr, entry: EntryInfo) -> io::Result<()> {
        Ok(())
    }

    async fn send_files(&self) -> io::Result<()> {
        while let Some((target, entry)) = self.transfer_chan.rx.lock().await.recv().await {}
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
