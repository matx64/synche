use crate::{
    application::{
        AppState, EntryManager, PeerManager,
        network::transport::interface::{TransportInterfaceV2, TransportRecvEvent},
        persistence::interface::PersistenceInterface,
    },
    domain::{
        EntryInfo, Peer, VersionCmp,
        transport::{TransportChannel, TransportChannelData, TransportDataV2},
    },
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::mpsc::Sender};
use tracing::{error, info, warn};

pub struct TransportReceiverV2<T: TransportInterfaceV2, P: PersistenceInterface> {
    adapter: Arc<T>,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_tx: Sender<TransportChannelData>,
    control_chan: TransportChannel<TransportRecvEvent>,
    transfer_chan: TransportChannel<TransportRecvEvent>,
}

impl<T: TransportInterfaceV2, P: PersistenceInterface> TransportReceiverV2<T, P> {
    pub fn new(
        adapter: Arc<T>,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        send_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            adapter,
            state,
            peer_manager,
            entry_manager,
            send_tx,
            control_chan: TransportChannel::new(),
            transfer_chan: TransportChannel::new(),
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.recv(), self.recv_control(), self.recv_transfer())?;
        Ok(())
    }

    async fn recv(&self) -> io::Result<()> {
        loop {
            let event = self.adapter.recv().await?;
            match event.data {
                TransportDataV2::Transfer(_) => {
                    self.transfer_chan
                        .tx
                        .send(event)
                        .await
                        .map_err(io::Error::other)?;
                }

                _ => {
                    self.control_chan
                        .tx
                        .send(event)
                        .await
                        .map_err(io::Error::other)?;
                }
            }
        }
    }

    async fn recv_transfer(&self) -> io::Result<()> {
        while let Some(event) = self.transfer_chan.rx.lock().await.recv().await {
            self.handle_transfer(event).await?;
        }
        Ok(())
    }

    async fn recv_control(&self) -> io::Result<()> {
        while let Some(event) = self.control_chan.rx.lock().await.recv().await {
            match event.data {
                TransportDataV2::HandshakeSyn(_) | TransportDataV2::HandshakeAck(_) => {
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
        let (hs_data, is_syn) = match event.data {
            TransportDataV2::HandshakeSyn(data) => (data, true),
            TransportDataV2::HandshakeAck(data) => (data, false),
            _ => unreachable!(),
        };

        let peer = Peer::new(event.src_id, event.src_ip, Some(hs_data.sync_dirs));
        self.peer_manager.insert(peer.clone());

        if is_syn {
            // Can't use send_tx because Response must be sent strictly BEFORE syncing
            let data = self.entry_manager.get_handshake_data().await;
            self.try_send(
                || {
                    self.adapter
                        .send(peer.addr, TransportDataV2::HandshakeAck(data.clone()))
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
                self.send_tx
                    .send(TransportChannelData::Request((peer.addr, entry)))
                    .await
                    .map_err(io::Error::other)?;
            } else {
                self.create_received_dir(entry).await?;
            }
        }
        Ok(())
    }

    async fn handle_metadata(&self, event: TransportRecvEvent) -> io::Result<()> {
        let peer_entry = match event.data {
            TransportDataV2::Metadata(entry) => entry,
            _ => unreachable!(),
        };

        match self
            .entry_manager
            .handle_metadata(event.src_id, &peer_entry)
            .await?
        {
            VersionCmp::KeepOther => {
                if peer_entry.is_removed() {
                    self.remove_entry(&peer_entry.name).await
                } else if peer_entry.is_file() {
                    self.send_tx
                        .send(TransportChannelData::Request((event.src_ip, peer_entry)))
                        .await
                        .map_err(io::Error::other)
                } else {
                    self.create_received_dir(peer_entry).await
                }
            }

            _ => Ok(()),
        }
    }

    async fn handle_request(&self, event: TransportRecvEvent) -> io::Result<()> {
        let requested_entry = match event.data {
            TransportDataV2::Request(entry) => entry,
            _ => unreachable!(),
        };

        match self.entry_manager.get_entry(&requested_entry.name).await {
            Some(local_entry)
                if local_entry.is_file()
                    && matches!(local_entry.compare(&requested_entry), VersionCmp::Equal) =>
            {
                self.send_tx
                    .send(TransportChannelData::Transfer((event.src_ip, local_entry)))
                    .await
                    .map_err(io::Error::other)
            }

            _ => Ok(()),
        }
    }

    async fn handle_transfer(&self, event: TransportRecvEvent) -> io::Result<()> {
        let received_entry = match event.data {
            TransportDataV2::Transfer(entry) => entry,
            _ => unreachable!(),
        };

        let entry = self.entry_manager.insert_entry(received_entry).await;

        self.send_tx
            .send(TransportChannelData::Metadata(entry))
            .await
            .map_err(io::Error::other)
    }

    async fn create_received_dir(&self, dir: EntryInfo) -> io::Result<()> {
        let dir = self.entry_manager.insert_entry(dir).await;

        let path = self.state.home_path.join(&*dir.name);
        fs::create_dir_all(path).await?;

        self.send_tx
            .send(TransportChannelData::Metadata(dir))
            .await
            .map_err(io::Error::other)
    }

    async fn remove_entry(&self, entry_name: &str) -> io::Result<()> {
        let _ = self.entry_manager.remove_entry(entry_name).await;

        let path = self.state.home_path.join(entry_name);

        if path.is_dir() {
            fs::remove_dir_all(path).await?;
        } else if path.is_file() {
            fs::remove_file(path).await?;
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

            if !self.peer_manager.exists(addr) {
                warn!("⚠️  Cancelled transport send op because peer disconnected during process.");
                return;
            }
        }

        error!(peer = ?addr, "Disconnecting peer after 3 Transport send attempts.");
        self.peer_manager.remove_peer_by_addr(addr);
    }
}
