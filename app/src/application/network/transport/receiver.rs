use crate::{
    application::{
        EntryManager, PeerManager, network::transport::interface::TransportInterface,
        persistence::interface::PersistenceInterface,
    },
    domain::{
        AppState, Channel, EntryInfo, Peer, TransportChannelData, TransportData, TransportEvent,
        VersionCmp,
    },
    utils::fs::home_dir,
};
use futures::TryFutureExt;
use std::{net::IpAddr, sync::Arc};
use tokio::{fs, io, sync::mpsc::Sender};
use tracing::{error, info, warn};

pub struct TransportReceiver<T: TransportInterface, P: PersistenceInterface> {
    adapter: Arc<T>,
    _state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    send_tx: Sender<TransportChannelData>,
    control_chan: Channel<TransportEvent>,
    transfer_chan: Channel<TransportEvent>,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportReceiver<T, P> {
    pub fn new(
        adapter: Arc<T>,
        _state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        send_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            adapter,
            _state,
            peer_manager,
            entry_manager,
            send_tx,
            control_chan: Channel::new(100),
            transfer_chan: Channel::new(16),
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::select!(
            res = self.recv() => res,
            res = self.recv_control() => res,
            res = self.recv_transfer() => res
        )
    }

    async fn recv(&self) -> io::Result<()> {
        loop {
            let event = self.adapter.recv().await?;
            match event.payload {
                TransportData::Transfer(_) => {
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
        warn!("Transport RECV Transfer channel closed");
        Ok(())
    }

    async fn recv_control(&self) -> io::Result<()> {
        while let Some(event) = self.control_chan.rx.lock().await.recv().await {
            match event.payload {
                TransportData::HandshakeSyn(_) | TransportData::HandshakeAck(_) => {
                    self.handle_handshake(event).await?;
                }

                TransportData::Metadata(_) => {
                    self.handle_metadata(event).await?;
                }

                TransportData::Request(_) => {
                    self.handle_request(event).await?;
                }

                _ => unreachable!(),
            }
        }
        warn!("Transport RECV Control channel closed");
        Ok(())
    }

    async fn handle_handshake(&self, event: TransportEvent) -> io::Result<()> {
        let (hs_data, is_syn) = match event.payload {
            TransportData::HandshakeSyn(data) => (data, true),
            TransportData::HandshakeAck(data) => (data, false),
            _ => unreachable!(),
        };

        let peer = Peer::new(
            event.metadata.source_id,
            event.metadata.source_ip,
            hs_data.hostname,
            hs_data.instance_id,
            hs_data.sync_dirs,
        );
        self.peer_manager.insert(peer.clone()).await;

        if is_syn {
            // Can't use send_tx because Response must be sent strictly BEFORE syncing
            let data = self.entry_manager.get_handshake_data().await?;
            self.try_send(
                || {
                    self.adapter
                        .send(peer.addr, TransportData::HandshakeAck(data.clone()))
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

    async fn handle_metadata(&self, event: TransportEvent) -> io::Result<()> {
        let peer_entry = match event.payload {
            TransportData::Metadata(entry) => entry,
            _ => unreachable!(),
        };

        match self
            .entry_manager
            .handle_metadata(event.metadata.source_id, &peer_entry)
            .await?
        {
            VersionCmp::KeepOther => {
                if peer_entry.is_removed() {
                    self.remove_entry(&peer_entry.name).await
                } else if peer_entry.is_file() {
                    self.send_tx
                        .send(TransportChannelData::Request((
                            event.metadata.source_ip,
                            peer_entry,
                        )))
                        .await
                        .map_err(io::Error::other)
                } else {
                    self.create_received_dir(peer_entry).await
                }
            }

            _ => Ok(()),
        }
    }

    async fn handle_request(&self, event: TransportEvent) -> io::Result<()> {
        let requested_entry = match event.payload {
            TransportData::Request(entry) => entry,
            _ => unreachable!(),
        };

        match self.entry_manager.get_entry(&requested_entry.name).await? {
            Some(local_entry)
                if local_entry.is_file()
                    && matches!(local_entry.compare(&requested_entry), VersionCmp::Equal) =>
            {
                self.send_tx
                    .send(TransportChannelData::Transfer((
                        event.metadata.source_ip,
                        local_entry,
                    )))
                    .await
                    .map_err(io::Error::other)
            }

            _ => Ok(()),
        }
    }

    async fn handle_transfer(&self, event: TransportEvent) -> io::Result<()> {
        let received_entry = match event.payload {
            TransportData::Transfer(entry) => entry,
            _ => unreachable!(),
        };

        let entry = self.entry_manager.insert_entry(received_entry).await?;

        self.send_tx
            .send(TransportChannelData::Metadata(entry))
            .await
            .map_err(io::Error::other)
    }

    async fn create_received_dir(&self, dir: EntryInfo) -> io::Result<()> {
        let dir = self.entry_manager.insert_entry(dir).await?;

        let path = home_dir().join(&*dir.name);
        fs::create_dir_all(path).await?;

        self.send_tx
            .send(TransportChannelData::Metadata(dir))
            .await
            .map_err(io::Error::other)
    }

    async fn remove_entry(&self, entry_name: &str) -> io::Result<()> {
        let _ = self.entry_manager.remove_entry(entry_name).await?;

        let path = home_dir().join(entry_name);

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

            if !self.peer_manager.exists(addr).await {
                warn!("⚠️  Cancelled transport send op because peer disconnected during process.");
                return;
            }
        }

        error!(peer = ?addr, "Disconnecting peer after 3 Transport send attempts.");
        self.peer_manager.remove_peer_by_addr(addr).await;
    }
}
