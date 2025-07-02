use crate::{Device, config::SynchedFile, file::send_file};
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
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
}

impl PresenceHandler {
    pub async fn new(
        devices: Arc<RwLock<HashMap<SocketAddr, Device>>>,
        synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    ) -> Self {
        let bind_addr = format!("0.0.0.0:{}", BROADCAST_PORT);

        let socket = Arc::new(UdpSocket::bind(&bind_addr).await.unwrap());
        socket.set_broadcast(true).unwrap();

        Self {
            socket,
            devices,
            synched_files,
        }
    }

    pub async fn send_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let broadcast_addr = format!("255.255.255.255:{}", BROADCAST_PORT);
        let mut retries: usize = 0;

        loop {
            if retries >= 3 {
                return Err(io::Error::other("Failed to send presence 3 times"));
            }

            let msg = match self.serialize_files() {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("Failed to serialize files: {}", e);
                    retries += 1;
                    continue;
                }
            };

            if let Err(e) = socket.send_to(msg.as_bytes(), &broadcast_addr).await {
                eprintln!("Error sending presence: {}", e);
            }

            retries = 0;
            tokio::time::sleep(Duration::from_secs(BROADCAST_INTERVAL_SECS)).await;
        }
    }

    pub async fn recv_presence(&self) -> io::Result<()> {
        let socket = self.socket.clone();
        let ifas = list_afinet_netifas().unwrap();

        let mut buf = [0; 1024];
        loop {
            let (size, src_addr) = socket.recv_from(&mut buf).await?;

            let _msg = String::from_utf8_lossy(&buf[..size]);
            if self.is_host(&ifas, src_addr.ip()) {
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

    pub async fn watch_devices(&self) -> io::Result<()> {
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
    }

    fn is_host(&self, ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
        ifas.iter().any(|ifa| ifa.1 == addr)
    }

    fn serialize_files(&self) -> Result<String, String> {
        match self.synched_files.read() {
            Ok(files) => {
                let vec = files.values().collect::<Vec<_>>();
                match serde_json::to_string(&vec) {
                    Ok(json) => Ok(json),
                    Err(err) => Err(err.to_string()),
                }
            }
            Err(err) => Err(err.to_string()),
        }
    }
}
