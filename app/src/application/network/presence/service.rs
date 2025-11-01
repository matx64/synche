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
                PresenceEvent::Ping { id, ip, instance_id } => {
                    self.handle_ping(id, ip, instance_id).await?;
                }

                PresenceEvent::Disconnect(peer_id) => {
                    self.handle_disconnect(peer_id).await?;
                }
            }
        }
        warn!("Presence adapter channel closed");
        Ok(())
    }

    async fn handle_ping(
        &self,
        peer_id: Uuid,
        peer_ip: IpAddr,
        instance_id: Uuid,
    ) -> io::Result<()> {
        let updated = self.peer_manager.update_if_exists(&peer_id, &instance_id).await;

        if !updated && self.state.local_id < peer_id {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(peer_ip))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    async fn handle_disconnect(&self, peer_id: Uuid) -> io::Result<()> {
        self.peer_manager.remove_peer(peer_id).await;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.adapter.shutdown().await;
    }
}
