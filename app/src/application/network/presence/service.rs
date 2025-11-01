use crate::{
    application::{
        network::presence::interface::{PresenceEvent, PresenceInterface},
        PeerManager,
    },
    domain::{AppState, TransportChannelData},
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::warn;
use uuid::Uuid;

pub struct PresenceService<P: PresenceInterface> {
    adapter: P,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    sender_tx: Sender<TransportChannelData>,
}

impl<P: PresenceInterface> PresenceService<P> {
    pub fn new(
        adapter: P,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        sender_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            state,
            adapter,
            sender_tx,
            peer_manager,
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        self.adapter.advertise().await?;

        while let Some(event) = self.adapter.next().await? {
            match event {
                PresenceEvent::Ping(id, addr) => {
                    self.handle_ping(id, addr).await?;
                }

                PresenceEvent::Disconnect(id) => {
                    self.handle_disconnect(id).await?;
                }
            }
        }
        warn!("Presence adapter channel closed");
        Ok(())
    }

    async fn handle_ping(
        &self,
        id: Uuid,
        addr: IpAddr,
    ) -> io::Result<()> {
        if self.state.local_id < id {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(addr))
                .await
                .map_err(io::Error::other)
        } else {
            Ok(())
        }
    }

    async fn handle_disconnect(&self, id: Uuid) -> io::Result<()> {
        self.peer_manager.remove_peer(id).await;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.adapter.shutdown().await;
    }
}
