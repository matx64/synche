use crate::{application::network::PresenceInterface, domain::PeerManager};
use local_ip_address::{list_afinet_netifas, local_ip};
use std::{net::IpAddr, sync::Arc, time::Duration};
use tokio::{io, time};
use tracing::{error, info};

pub struct PresenceService<T: PresenceInterface> {
    presence_adapter: T,
    peer_manager: Arc<PeerManager>,
    broadcast_interval_secs: u64,
}

impl<T: PresenceInterface> PresenceService<T> {
    pub fn new(
        presence_adapter: T,
        peer_manager: Arc<PeerManager>,
        broadcast_interval_secs: u64,
    ) -> Self {
        Self {
            presence_adapter,
            peer_manager,
            broadcast_interval_secs,
        }
    }

    pub async fn run_broadcast(&self) -> io::Result<()> {
        let msg = "ping".as_bytes();
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

            if self.is_host(&ifas, src_ip) || msg != "ping" {
                continue;
            }

            let _send_handshake = self.peer_manager.insert_or_update(src_addr) && local_ip < src_ip;

            // TODO: Send Handshake
        }
    }

    pub async fn monitor_peers(&self) -> io::Result<()> {
        loop {
            info!("Connected Peers: {:?}", self.peer_manager.retain());
            time::sleep(Duration::from_secs(10)).await;
        }
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }
}
