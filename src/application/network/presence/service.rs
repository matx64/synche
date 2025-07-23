use crate::{
    application::network::PresenceInterface, domain::PeerManager,
    proto::transport::SyncHandshakeKind,
};
use local_ip_address::{list_afinet_netifas, local_ip};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{io, sync::mpsc::Sender, time};
use tracing::{error, info};
use uuid::Uuid;

pub struct PresenceService<T: PresenceInterface> {
    presence_adapter: T,
    local_id: Uuid,
    peer_manager: Arc<PeerManager>,
    handshake_tx: Sender<(SocketAddr, SyncHandshakeKind)>,
    broadcast_interval_secs: u64,
}

impl<T: PresenceInterface> PresenceService<T> {
    pub fn new(
        presence_adapter: T,
        local_id: Uuid,
        peer_manager: Arc<PeerManager>,
        handshake_tx: Sender<(SocketAddr, SyncHandshakeKind)>,
        broadcast_interval_secs: u64,
    ) -> Self {
        Self {
            presence_adapter,
            local_id,
            peer_manager,
            handshake_tx,
            broadcast_interval_secs,
        }
    }

    pub async fn run_broadcast(&self) -> io::Result<()> {
        let msg = format!("ping:{}", &self.local_id);
        let msg = msg.as_bytes();
        let mut retries: usize = 0;

        loop {
            if let Err(e) = self.presence_adapter.broadcast(msg).await {
                error!("Error sending presence: {}", e);
                retries += 1;

                if retries >= 3 {
                    return Err(io::Error::other("Failed to send presence 3 times"));
                }
            } else {
                retries = 0;
            }

            time::sleep(Duration::from_secs(self.broadcast_interval_secs)).await;
        }
    }

    pub async fn run_recv(&self) -> io::Result<()> {
        let local_ip = local_ip().unwrap();
        let ifas = list_afinet_netifas().unwrap();

        loop {
            let (msg, src_addr) = self.presence_adapter.recv().await?;
            let src_ip = src_addr.ip();

            if self.is_host(&ifas, src_ip) {
                continue;
            }

            let peer_id = msg
                .strip_prefix("ping:")
                .and_then(|id| Uuid::parse_str(id).ok());

            let Some(peer_id) = peer_id else {
                error!("Invalid or missing broadcast ID: {}", msg);
                continue;
            };

            let send_handshake =
                self.peer_manager.insert_or_update(peer_id, src_addr) && local_ip < src_ip;

            if send_handshake {
                self.handshake_tx
                    .send((src_addr, SyncHandshakeKind::Request))
                    .await
                    .map_err(io::Error::other)?;
            }
        }
    }

    pub async fn monitor_peers(&self) -> io::Result<()> {
        loop {
            info!("🖥️ Connected Peers: {:?}", self.peer_manager.retain());
            time::sleep(Duration::from_secs(10)).await;
        }
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }
}
