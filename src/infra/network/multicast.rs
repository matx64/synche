use crate::application::network::{PresenceInterface, presence::interface::PresenceResult};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;

const UDP_PORT: u16 = 8888;
const MULTICAST_ADDR_V4: &str = "239.255.0.1";

pub struct UdpMulticaster {
    socket: UdpSocket,
    multicast_addr: String,
}

impl UdpMulticaster {
    pub async fn new() -> Self {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();

        socket.set_reuse_address(true).unwrap();
        #[cfg(unix)]
        sock.set_reuse_port(true).unwrap();

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), UDP_PORT);
        socket.bind(&SockAddr::from(bind_addr)).unwrap();

        let std_udp: std::net::UdpSocket = socket.into();
        std_udp.set_nonblocking(true).unwrap();

        let maddr: Ipv4Addr = MULTICAST_ADDR_V4.parse().unwrap();
        std_udp
            .join_multicast_v4(&maddr, &Ipv4Addr::UNSPECIFIED)
            .unwrap();
        std_udp.set_multicast_loop_v4(true).unwrap();
        std_udp.set_multicast_ttl_v4(1).unwrap();

        let socket = UdpSocket::from_std(std_udp).unwrap();

        Self {
            socket,
            multicast_addr: format!("{}:{}", MULTICAST_ADDR_V4, UDP_PORT),
        }
    }
}

impl PresenceInterface for UdpMulticaster {
    async fn broadcast(&self, data: &[u8]) -> PresenceResult<()> {
        self.socket
            .send_to(data, &self.multicast_addr)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn recv(&self) -> PresenceResult<(String, IpAddr)> {
        let mut buf = vec![0u8; 1500];
        let (size, src_addr) = self
            .socket
            .recv_from(&mut buf)
            .await
            .map_err(|e| e.to_string())?;

        let msg = String::from_utf8_lossy(&buf[..size]).into_owned();

        Ok((msg, src_addr.ip()))
    }
}
