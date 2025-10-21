use crate::{
    application::{
        AppState, PeerManager,
        network::presence::interface::{PresenceEvent, PresenceInterface},
    },
    domain::TransportChannelData,
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
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
        self.adapter.advertise().await;

        loop {
            match self.adapter.recv().await? {
                PresenceEvent::Ping((peer_id, peer_ip)) => {
                    self.handle_peer_connect(peer_id, peer_ip).await?;
                }

                PresenceEvent::Disconnect(peer_id) => {
                    self.handle_peer_disconnect(peer_id).await?;
                }
            }
        }
    }

    async fn handle_peer_connect(&self, peer_id: Uuid, peer_ip: IpAddr) -> io::Result<()> {
        let inserted = self.peer_manager.insert_or_update(peer_id, peer_ip);

        if inserted && self.state.local_id < peer_id {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(peer_ip))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    async fn handle_peer_disconnect(&self, peer_id: Uuid) -> io::Result<()> {
        self.peer_manager.remove_peer(peer_id);
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.adapter.shutdown().await;
    }
}
