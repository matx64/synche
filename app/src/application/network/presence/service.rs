use crate::{
    application::{
        PeerManager,
        network::presence::interface::{PresenceEvent, PresenceInterface},
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
                PresenceEvent::Ping {
                    id,
                    addr,
                    instance_id,
                } => {
                    self.handle_ping(id, addr, instance_id).await?;
                }

                PresenceEvent::Disconnect(id) => {
                    self.handle_disconnect(id).await?;
                }
            }
        }
        warn!("Presence adapter channel closed");
        Ok(())
    }

    async fn handle_ping(&self, id: Uuid, addr: IpAddr, instance_id: Uuid) -> io::Result<()> {
        let seen = self.peer_manager.seen(&id, &instance_id).await;

        if !seen && self.state.local_id < id {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(addr))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    async fn handle_disconnect(&self, id: Uuid) -> io::Result<()> {
        self.peer_manager.remove_peer(id).await;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.adapter.shutdown().await;
    }
}
