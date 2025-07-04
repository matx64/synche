use crate::{Device, config::SynchedFile, file::send_file};
use local_ip_address::list_afinet_netifas;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};
use tokio::{io, net::UdpSocket};

const BROADCAST_PORT: u16 = 8888;
const BROADCAST_INTERVAL_SECS: u64 = 5;
const DEVICE_TIMEOUT_SECS: u64 = 15;

pub struct PresenceHandler {
    socket: Arc<UdpSocket>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    send_serialized_files: Arc<RwLock<bool>>,
}

impl PresenceHandler {
    pub async fn new(
        devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
        synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    ) -> Self {
        let bind_addr = format!("0.0.0.0:{}", BROADCAST_PORT);

        let socket = Arc::new(UdpSocket::bind(&bind_addr).await.unwrap());
        socket.set_broadcast(true).unwrap();

        Self {
            socket,
            devices,
            synched_files,
            send_serialized_files: Arc::new(RwLock::new(false)),
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

            let send_serialized_files = self.send_serialized_files.read().is_ok_and(|val| *val);

            let msg = if !send_serialized_files {
                "ping"
            } else {
                &match self.serialize_files() {
                    Ok(json) => json,
                    Err(e) => {
                        eprintln!("Failed to serialize files: {}", e);
                        retries += 1;
                        continue;
                    }
                }
            };

            if let Err(e) = socket.send_to(msg.as_bytes(), &broadcast_addr).await {
                eprintln!("Error sending presence: {}", e);
                retries += 1;
            } else {
                if send_serialized_files {
                    if let Ok(mut val) = self.send_serialized_files.write() {
                        *val = false;
                    }
                }
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

            if self.is_host(&ifas, ip) {
                continue;
            }

            let msg = String::from_utf8_lossy(&buf[..size]);

            if msg == "ping" {
                if let Ok(mut devices) = self.devices.write() {
                    if let Some(device) = devices.get_mut(&ip) {
                        device.last_seen = SystemTime::now();
                    } else {
                        devices.insert(ip, Device::new(src_addr, None));

                        if let Ok(mut val) = self.send_serialized_files.write() {
                            *val = true;
                        }
                    }
                }
                continue;
            }

            let device_synched_files = match self.deserialize_files(&msg) {
                Ok(map) => map,
                Err(err) => {
                    eprintln!("Failed to deserialize msg from {}: {}", src_addr, err);
                    continue;
                }
            };

            let sync_devices = if let Ok(mut devices) = self.devices.write() {
                if devices
                    .insert(ip, Device::new(src_addr, Some(device_synched_files)))
                    .is_none()
                {
                    println!("Device connected: {}", src_addr);
                    if let Ok(mut val) = self.send_serialized_files.write() {
                        *val = true;
                    }
                }
                true
            } else {
                false
            };

            if sync_devices {
                self.sync_devices(src_addr).await;
            }
        }
    }

    async fn sync_devices(&self, other: SocketAddr) {
        let other_device = if let Ok(devices) = self.devices.read() {
            if let Some(device) = devices.get(&other.ip()) {
                device.clone()
            } else {
                return;
            }
        } else {
            eprintln!("Failed to read devices");
            return;
        };

        let files_to_send = if let Ok(files) = self.synched_files.read() {
            files
                .values()
                .filter_map(|f| {
                    other_device
                        .synched_files
                        .get(&f.name)
                        .filter(|d| d.last_modified_at < f.last_modified_at)
                        .cloned()
                })
                .collect::<Vec<SynchedFile>>()
        } else {
            eprintln!("Failed to read synched files");
            return;
        };

        for file in files_to_send {
            if send_file(&file, other).await.is_err() {
                eprintln!("Failed to send file {} to {}", file.name, other);
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

    fn deserialize_files(&self, msg: &str) -> Result<HashMap<String, SynchedFile>, String> {
        match serde_json::from_str::<Vec<SynchedFile>>(msg) {
            Ok(files) => Ok(files
                .into_iter()
                .map(|f| (f.name.clone(), f))
                .collect::<HashMap<String, SynchedFile>>()),
            Err(err) => Err(err.to_string()),
        }
    }
}
