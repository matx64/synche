use crate::{
    config::AppState,
    models::{device::Device, sync::SyncDataKind},
    services::file::FileService,
};
use std::{io::ErrorKind, net::SocketAddr, sync::Arc};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::{error, info};

pub struct HandshakeService {
    state: Arc<AppState>,
    file_service: Arc<FileService>,
}

impl HandshakeService {
    pub fn new(state: Arc<AppState>, file_service: Arc<FileService>) -> Self {
        Self {
            state,
            file_service,
        }
    }

    pub async fn send_handshake(
        &self,
        mut target_addr: SocketAddr,
        is_request: bool,
    ) -> io::Result<()> {
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        let contents = self.state.entry_manager.serialize()?;

        let kind = if is_request {
            SyncDataKind::HandshakeRequest
        } else {
            SyncDataKind::HandshakeResponse
        };

        let content_b = contents.as_bytes();
        let content_len = content_b.len() as u32;

        info!("Sending {} to {}", kind, target_addr.ip());

        stream.write_all(&[kind as u8]).await?;
        stream.write_all(&content_len.to_be_bytes()).await?;
        stream.write_all(content_b).await?;

        Ok(())
    }

    pub async fn read_handshake(&self, stream: &mut TcpStream, is_request: bool) -> io::Result<()> {
        let src_addr = stream.peer_addr()?;
        let ip = src_addr.ip();

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len_buf = u32::from_be_bytes(len_buf) as usize;

        let mut content_buf = vec![0u8; len_buf];
        stream.read_exact(&mut content_buf).await?;
        let content = String::from_utf8(content_buf)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        let entries = self.state.entry_manager.deserialize(&content)?;

        if let Ok(mut devices) = self.state.devices.write() {
            if devices
                .insert(ip, Device::new(src_addr, Some(entries)))
                .is_none()
            {
                info!("Device connected: {}", ip);
            }
        }

        if is_request {
            self.send_handshake(src_addr, false).await?;
        }

        self.sync_devices(src_addr).await;

        Ok(())
    }

    async fn sync_devices(&self, other: SocketAddr) {
        let ip = other.ip();
        let other_device = if let Ok(devices) = self.state.devices.read() {
            if let Some(device) = devices.get(&ip) {
                device.clone()
            } else {
                return;
            }
        } else {
            error!("Failed to read devices");
            return;
        };

        info!("Synching device: {}", ip);

        let files_to_send = self.state.entry_manager.to_send(&other_device);

        for file in files_to_send {
            if let Err(err) = self.file_service.send_file(&file, other).await {
                error!("Failed to send file {} to {}: {}", file.name, other, err);
            }
        }
    }
}
