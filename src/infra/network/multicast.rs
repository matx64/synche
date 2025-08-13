use crate::application::network::{PresenceInterface, presence::interface::PresenceResult};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;
use tracing::warn;

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
        socket.set_reuse_port(true).unwrap();

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), UDP_PORT);
        socket.bind(&SockAddr::from(bind_addr)).unwrap();

        let socket: std::net::UdpSocket = socket.into();
        socket.set_nonblocking(true).unwrap();

        let maddr: Ipv4Addr = MULTICAST_ADDR_V4.parse().unwrap();
        let ifas = local_ip_address::list_afinet_netifas().unwrap();

        let mut joined_any = false;
        for (name, ip) in ifas {
            if let IpAddr::V4(ipv4) = ip {
                if ipv4.is_loopback() || name.to_lowercase().contains("bluetooth") {
                    continue;
                }

                if let Err(err) = socket.join_multicast_v4(&maddr, &ipv4) {
                    warn!("Failed to join multicast in ({name}, {ipv4}) ifa: {err}");
                } else {
                    joined_any = true;
                }
            }
        }

        if !joined_any {
            panic!("Couldn't join multicast in any interface!");
        }

        socket.set_multicast_loop_v4(true).unwrap();
        socket.set_multicast_ttl_v4(1).unwrap();

        let socket = UdpSocket::from_std(socket).unwrap();

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
