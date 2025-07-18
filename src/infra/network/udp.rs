use crate::application::network::BroadcastPort;
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

impl BroadcastPort for UdpBroadcaster {
    async fn broadcast(&self, data: &[u8]) -> tokio::io::Result<()> {
        self.socket
            .send_to(data, &self.broadcast_addr)
            .await
            .map(|_| ())
    }

    async fn recv(&self) -> tokio::io::Result<(Vec<u8>, SocketAddr)> {
        let mut buf = vec![0u8; 8];
        let (_, src_addr) = self.socket.recv_from(&mut buf).await?;
        Ok((buf, src_addr))
    }
}
