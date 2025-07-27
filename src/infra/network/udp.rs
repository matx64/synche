use crate::application::network::PresenceInterface;
use std::net::IpAddr;
use tokio::net::UdpSocket;

const UDP_PORT: u16 = 8888;

pub struct UdpBroadcaster {
    socket: UdpSocket,
    broadcast_addr: String,
}

impl UdpBroadcaster {
    pub async fn new() -> Self {
        let bind_addr = format!("0.0.0.0:{UDP_PORT}");
        let broadcast_addr = format!("255.255.255.255:{UDP_PORT}");

        let socket = UdpSocket::bind(&bind_addr).await.unwrap();
        socket.set_broadcast(true).unwrap();

        Self {
            socket,
            broadcast_addr,
        }
    }
}

impl PresenceInterface for UdpBroadcaster {
    async fn broadcast(&self, data: &[u8]) -> tokio::io::Result<()> {
        self.socket
            .send_to(data, &self.broadcast_addr)
            .await
            .map(|_| ())
    }

    async fn recv(&self) -> tokio::io::Result<(String, IpAddr)> {
        let mut buf = vec![0u8; 50];
        let (size, src_addr) = self.socket.recv_from(&mut buf).await?;

        let msg = String::from_utf8_lossy(&buf[..size]).into_owned();

        Ok((msg, src_addr.ip()))
    }
}
