use crate::application::network::PresenceInterface;
use local_ip_address::{list_afinet_netifas, local_ip};
use std::{net::IpAddr, time::Duration};
use tokio::{io, time};
use tracing::error;

pub struct PresenceService<T: PresenceInterface> {
    presence_adapter: T,
    broadcast_interval_secs: u64,
}

impl<T: PresenceInterface> PresenceService<T> {
    pub fn new(presence_adapter: T, broadcast_interval_secs: u64) -> Self {
        Self {
            presence_adapter,
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

            // TODO: handle peer + handshake
        }
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }
}
