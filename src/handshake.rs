use crate::{Device, config::SynchedFile, file::send_file, sync::SyncDataKind};
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    io::ErrorKind,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const TCP_PORT: u16 = 8889;

pub struct HandshakeHandler {
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    hash: u64,
}

impl HandshakeHandler {
    pub fn new(
        devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
        synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    ) -> Self {
        Self {
            devices,
            synched_files,
            hash: Self::handshake_hash(),
        }
    }

    pub async fn send_handshake(&self, mut target_addr: SocketAddr) -> io::Result<()> {
        target_addr.set_port(TCP_PORT);
        let mut stream = TcpStream::connect(target_addr).await?;

        let contents = match self.serialize_files() {
            Ok(json) => json,
            Err(err) => {
                eprintln!("Failed to serialize files: {}", err);
                return Err(io::Error::other(err));
            }
        };

        let kind = SyncDataKind::Handshake as u8;
        let hash = self.hash.to_be_bytes();
        let content_b = contents.as_bytes();
        let content_len = content_b.len() as u32;

        stream.write_all(&[kind]).await?;
        stream.write_all(&hash).await?;
        stream.write_all(&content_len.to_be_bytes()).await?;
        stream.write_all(content_b).await?;

        Ok(())
    }

    pub async fn read_handshake(&self, stream: &mut TcpStream) -> io::Result<()> {
        let src_addr = stream.peer_addr()?;

        let mut hash_buf = [0u8; 8];
        stream.read_exact(&mut hash_buf).await?;
        let handshake_hash = u64::from_be_bytes(hash_buf);

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len_buf = u32::from_be_bytes(len_buf) as usize;

        let mut content_buf = vec![0u8; len_buf];
        stream.read_exact(&mut content_buf).await?;
        let content = String::from_utf8(content_buf)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        let synched_files = self.deserialize_files(&content)?;

        let send_handshake = if let Ok(mut devices) = self.devices.write() {
            if let Some(device) = devices.get_mut(&src_addr.ip()) {
                device.synched_files = synched_files;

                match device.handshake_hash {
                    Some(hash) if hash != handshake_hash => {
                        device.handshake_hash = Some(handshake_hash);
                        true
                    }
                    _ => false,
                }
            } else {
                true
            }
        } else {
            false
        };

        if send_handshake {
            self.send_handshake(src_addr).await?;
        }

        self.sync_devices(src_addr).await;

        Ok(())
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

    fn deserialize_files(&self, msg: &str) -> io::Result<HashMap<String, SynchedFile>> {
        match serde_json::from_str::<Vec<SynchedFile>>(msg) {
            Ok(files) => Ok(files
                .into_iter()
                .map(|f| (f.name.clone(), f))
                .collect::<HashMap<String, SynchedFile>>()),
            Err(err) => Err(io::Error::new(ErrorKind::InvalidData, err)),
        }
    }

    fn handshake_hash() -> u64 {
        let boot_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut hasher = DefaultHasher::new();
        boot_time.hash(&mut hasher);
        hasher.finish()
    }
}
