use crate::{
    application::PeerManager, infra::network::mdns::MdnsAdapter,
    proto::transport::SyncHandshakeKind,
};
use mdns_sd::{ServiceEvent, ServiceInfo};
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::{info, warn};
use uuid::Uuid;

pub struct PresenceService {
    mdns_adapter: MdnsAdapter,
    local_id: Uuid,
    peer_manager: Arc<PeerManager>,
    handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
}

impl PresenceService {
    pub fn new(
        local_id: Uuid,
        peer_manager: Arc<PeerManager>,
        handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
    ) -> Self {
        Self {
            mdns_adapter: MdnsAdapter::new(local_id),
            local_id,
            peer_manager,
            handshake_tx,
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        self.mdns_adapter.advertise();

        loop {
            match self.mdns_adapter.recv().await? {
                ServiceEvent::ServiceResolved(info) => {
                    self.handle_peer_connect(info).await?;
                }

                ServiceEvent::ServiceRemoved(_, fullname) => {
                    self.handle_peer_disconnect(&fullname).await?;
                }

                _ => {
                    info!("🖥️  Connected Peers: {:?}", self.peer_manager.list());
                }
            }
        }
    }

    async fn handle_peer_connect(&self, info: ServiceInfo) -> io::Result<()> {
        let Some(peer_id) = self.mdns_adapter.get_peer_id(info.get_fullname()) else {
            return Ok(());
        };

        if peer_id == self.local_id {
            return Ok(());
        }

        if let Some(peer_ip) = info.get_addresses().iter().next().cloned() {
            let inserted = self.peer_manager.insert_or_update(peer_id, peer_ip);

            if inserted && self.local_id < peer_id {
                self.handshake_tx
                    .send((peer_ip, SyncHandshakeKind::Request))
                    .await
                    .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }

    async fn handle_peer_disconnect(&self, fullname: &str) -> io::Result<()> {
        let Some(peer_id) = self.mdns_adapter.get_peer_id(fullname) else {
            warn!(fullname = fullname, "Invalid mDNS peer id");
            return Ok(());
        };

        self.peer_manager.remove_peer(peer_id);
        Ok(())
    }

    pub fn shutdown(&self) {
        self.mdns_adapter.shutdown();
    }
}
