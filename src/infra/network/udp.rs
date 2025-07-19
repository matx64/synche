use crate::application::network::PresenceInterface;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

pub struct UdpBroadcaster {
    socket: UdpSocket,
    broadcast_addr: String,
}

impl UdpBroadcaster {
    pub async fn new(port: u16) -> Self {
        let bind_addr = format!("0.0.0.0:{port}");
        let broadcast_addr = format!("255.255.255.255:{port}");

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

    async fn recv(&self) -> tokio::io::Result<(String, SocketAddr)> {
        let mut buf = vec![0u8; 8];
        let (size, src_addr) = self.socket.recv_from(&mut buf).await?;

        let msg = String::from_utf8_lossy(&buf[..size]).into_owned();

        Ok((msg, src_addr))
    }
}
