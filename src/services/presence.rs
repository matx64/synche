use crate::{config::AppState, models::peer::Peer, services::handshake::HandshakeService};
use local_ip_address::{list_afinet_netifas, local_ip};
use std::{net::IpAddr, sync::Arc, time::Duration};
use tokio::{io, net::UdpSocket};
use tracing::{error, info};

pub struct PresenceService {
    state: Arc<AppState>,
    socket: Arc<UdpSocket>,
    handshake_service: Arc<HandshakeService>,
}

impl PresenceService {
    pub async fn new(state: Arc<AppState>, handshake_service: Arc<HandshakeService>) -> Self {
        let bind_addr = format!("0.0.0.0:{}", state.constants.broadcast_port);

        let socket = Arc::new(UdpSocket::bind(&bind_addr).await.unwrap());
        socket.set_broadcast(true).unwrap();

        Self {
            state,
            socket,
            handshake_service,
        }
    }

    pub async fn send_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let broadcast_addr = format!("255.255.255.255:{}", self.state.constants.broadcast_port);
        let mut retries: usize = 0;

        loop {
            if let Err(e) = socket.send_to("ping".as_bytes(), &broadcast_addr).await {
                error!("Error sending presence: {}", e);
                retries += 1;

                if retries >= 3 {
                    return Err(io::Error::other("Failed to send presence 3 times"));
                }
            } else {
                retries = 0;
            }

            tokio::time::sleep(Duration::from_secs(
                self.state.constants.broadcast_interval_secs,
            ))
            .await;
        }
    }

    pub async fn recv_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let local_ip = local_ip().unwrap();
        let ifas = list_afinet_netifas().unwrap();

        let mut buf = [0u8; 8];
        loop {
            let (size, src_addr) = socket.recv_from(&mut buf).await?;
            let src_ip = src_addr.ip();

            let msg = String::from_utf8_lossy(&buf[..size]);
            if self.is_host(&ifas, src_ip) || msg != "ping" {
                continue;
            }

            let send_handshake = self
                .state
                .peer_manager
                .insert_or_update(Peer::new(src_addr, None))
                && local_ip < src_ip;

            if send_handshake {
                self.handshake_service
                    .send_handshake(src_addr, true)
                    .await?;
            }
        }
    }

    pub async fn watch_peers(&self) -> io::Result<()> {
        info!(
            "🚀 Synche running on port {}. Press Ctrl+C to stop.",
            self.state.constants.broadcast_port
        );

        loop {
            info!(
                "Connected Synche peers: {}",
                self.state.peer_manager.retain()
            );
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }
}
