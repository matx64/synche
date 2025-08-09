use crate::{
    application::{PeerManager, network::PresenceInterface},
    proto::transport::SyncHandshakeKind,
};
use local_ip_address::list_afinet_netifas;
use std::{net::IpAddr, sync::Arc, time::Duration};
use tokio::{io, sync::mpsc::Sender, time};
use tracing::{error, info};
use uuid::Uuid;

pub struct PresenceService<T: PresenceInterface> {
    presence_adapter: T,
    local_id: Uuid,
    peer_manager: Arc<PeerManager>,
    handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
    broadcast_msg: Vec<u8>,
    broadcast_interval_secs: u64,
}

impl<T: PresenceInterface> PresenceService<T> {
    pub fn new(
        presence_adapter: T,
        local_id: Uuid,
        peer_manager: Arc<PeerManager>,
        handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
        broadcast_interval_secs: u64,
    ) -> Self {
        Self {
            broadcast_msg: format!("ping:{}", &local_id).as_bytes().to_vec(),
            presence_adapter,
            local_id,
            peer_manager,
            handshake_tx,
            broadcast_interval_secs,
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.run_recv(), self.run_broadcast(), self.monitor_peers())?;
        Ok(())
    }

    async fn run_broadcast(&self) -> io::Result<()> {
        let mut retries: usize = 0;
        let mut interval = time::interval(Duration::from_secs(self.broadcast_interval_secs));

        loop {
            if let Err(e) = self.presence_adapter.broadcast(&self.broadcast_msg).await {
                error!("Error sending presence: {}", e);
                retries += 1;

                if retries >= 3 {
                    return Err(io::Error::other("Failed to send presence 3 times in a row"));
                }
            } else {
                retries = 0;
            }
            interval.tick().await;
        }
    }

    async fn run_recv(&self) -> io::Result<()> {
        let ifas = list_afinet_netifas().unwrap();

        loop {
            let (msg, src_ip) = self.presence_adapter.recv().await?;

            if self.is_host(&ifas, src_ip) {
                continue;
            }

            let is_ping = msg.starts_with("ping:");

            let peer_id = if is_ping {
                msg.strip_prefix("ping:")
                    .and_then(|id| Uuid::parse_str(id).ok())
            } else {
                msg.strip_prefix("shutdown:")
                    .and_then(|id| Uuid::parse_str(id).ok())
            };

            let Some(peer_id) = peer_id else {
                error!("Invalid or missing broadcast ID: {}", msg);
                continue;
            };

            if !is_ping {
                self.peer_manager.remove_peer(peer_id);
                continue;
            }

            let send_handshake =
                self.peer_manager.insert_or_update(peer_id, src_ip) && self.local_id < peer_id;

            if send_handshake {
                self.handshake_tx
                    .send((src_ip, SyncHandshakeKind::Request))
                    .await
                    .map_err(io::Error::other)?;
            }
        }
    }

    async fn monitor_peers(&self) -> io::Result<()> {
        loop {
            info!("🖥️  Connected Peers: {:?}", self.peer_manager.retain());
            time::sleep(Duration::from_secs(10)).await;
        }
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], ip: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == ip)
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.broadcast_msg = format!("shutdown:{}", self.local_id).as_bytes().to_vec();
        self.presence_adapter.broadcast(&self.broadcast_msg).await
    }
}
