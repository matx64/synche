use crate::{Device, file::send_file};
use local_ip_address::list_afinet_netifas;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{io, net::UdpSocket};

const BROADCAST_PORT: u16 = 8888;
const BROADCAST_INTERVAL_SECS: u64 = 5;
const DEVICE_TIMEOUT_SECS: u64 = 15;
const SEND_FILE: bool = true;

pub struct PresenceHandler {
    socket: Arc<UdpSocket>,
    devices: Arc<RwLock<HashMap<SocketAddr, Device>>>,
}

impl PresenceHandler {
    pub async fn new(devices: Arc<RwLock<HashMap<SocketAddr, Device>>>) -> Self {
        let bind_addr = format!("0.0.0.0:{}", BROADCAST_PORT);

        let socket = Arc::new(UdpSocket::bind(&bind_addr).await.unwrap());
        socket.set_broadcast(true).unwrap();

        Self { socket, devices }
    }

    pub async fn send_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let broadcast_addr = format!("255.255.255.255:{}", BROADCAST_PORT);

        loop {
            socket.send_to("ping".as_bytes(), &broadcast_addr).await?;
            tokio::time::sleep(Duration::from_secs(BROADCAST_INTERVAL_SECS)).await;
        }
    }

    pub async fn recv_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let ifas = list_afinet_netifas().unwrap();

        let mut buf = [0; 1024];
        loop {
            let (size, src_addr) = socket.recv_from(&mut buf).await?;

            let msg = String::from_utf8_lossy(&buf[..size]);
            if self.is_host(&ifas, src_addr.ip()) || msg != "ping" {
                continue;
            }

            let mut should_send_file = false;
            {
                let mut devices = self.devices.write().unwrap();
                if devices.insert(src_addr, Device::new(src_addr)).is_none() {
                    println!("Device connected: {}", src_addr);
                    should_send_file = SEND_FILE;
                }
            }

            if should_send_file {
                send_file("file.txt", src_addr).await?;
            }
        }
    }

    pub async fn state(&self) -> io::Result<()> {
        println!(
            "🚀 Synche running on port {}. Press Ctrl+C to stop.",
            BROADCAST_PORT
        );

        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            let mut devices = self.devices.write().unwrap();
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
        Ok(())
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }
}
