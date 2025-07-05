use crate::{Device, config::SynchedFile, handshake::HandshakeHandler};
use local_ip_address::list_afinet_netifas;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};
use tokio::{io, net::UdpSocket};

const BROADCAST_PORT: u16 = 8888;
const BROADCAST_INTERVAL_SECS: u64 = 5;
const DEVICE_TIMEOUT_SECS: u64 = 15;

pub struct PresenceHandler {
    socket: Arc<UdpSocket>,
    handshake_handler: Arc<HandshakeHandler>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
}

impl PresenceHandler {
    pub async fn new(
        handshake_handler: Arc<HandshakeHandler>,
        devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
    ) -> Self {
        let bind_addr = format!("0.0.0.0:{}", BROADCAST_PORT);

        let socket = Arc::new(UdpSocket::bind(&bind_addr).await.unwrap());
        socket.set_broadcast(true).unwrap();

        Self {
            socket,
            handshake_handler,
            devices,
        }
    }

    pub async fn send_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let broadcast_addr = format!("255.255.255.255:{}", BROADCAST_PORT);
        let mut retries: usize = 0;

        loop {
            if let Err(e) = socket.send_to("ping".as_bytes(), &broadcast_addr).await {
                eprintln!("Error sending presence: {}", e);
                retries += 1;

                if retries >= 3 {
                    return Err(io::Error::other("Failed to send presence 3 times"));
                }
            } else {
                retries = 0;
            }

            tokio::time::sleep(Duration::from_secs(BROADCAST_INTERVAL_SECS)).await;
        }
    }

    pub async fn recv_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let ifas = list_afinet_netifas().unwrap();

        let mut buf = [0; 1024];
        loop {
            let (size, src_addr) = socket.recv_from(&mut buf).await?;
            let ip = src_addr.ip();

            let msg = String::from_utf8_lossy(&buf[..size]);
            if self.is_host(&ifas, ip) || msg != "ping" {
                continue;
            }

            let send_handshake = self.devices.write().is_ok_and(|mut devices| {
                if let Some(device) = devices.get_mut(&ip) {
                    device.last_seen = SystemTime::now();
                    false
                } else {
                    println!("Device connected: {}", ip);
                    devices.insert(ip, Device::new(src_addr, None));
                    true
                }
            });

            if send_handshake {
                self.handshake_handler.send_handshake(src_addr).await?;
            }
        }
    }

    pub async fn watch_devices(&self) -> io::Result<()> {
        println!(
            "🚀 Synche running on port {}. Press Ctrl+C to stop.",
            BROADCAST_PORT
        );

        loop {
            match self.devices.write() {
                Ok(mut devices) => {
                    devices.retain(|_, device| !matches!(device.last_seen.elapsed(), Ok(elapsed) if elapsed.as_secs() > DEVICE_TIMEOUT_SECS));

                    if !devices.is_empty() {
                        println!(
                            "Connected Synche devices: {:?}",
                            devices.keys().collect::<Vec<_>>()
                        );
                    } else {
                        println!("No Synche devices connected.");
                    }
                }
                Err(_) => continue,
            };

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }
}
