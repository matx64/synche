use crate::{
    config::AppState,
    file::FileService,
    handshake::HandshakeService,
    models::{file::SynchedFile, sync::SyncDataKind},
};
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::Receiver,
    time,
};
use tracing::{error, info};

pub struct SyncService {
    state: Arc<AppState>,
    file_service: Arc<FileService>,
    handshake_service: Arc<HandshakeService>,
}

impl SyncService {
    pub fn new(
        state: Arc<AppState>,
        file_service: Arc<FileService>,
        handshake_service: Arc<HandshakeService>,
    ) -> Self {
        Self {
            state,
            file_service,
            handshake_service,
        }
    }

    pub async fn recv_data(&self) -> std::io::Result<()> {
        let listener =
            TcpListener::bind(format!("0.0.0.0:{}", self.state.constants.tcp_port)).await?;

        loop {
            let (mut stream, addr) = listener.accept().await?;

            let mut kind_buf = [0u8; 1];
            stream.read_exact(&mut kind_buf).await?;

            let kind = SyncDataKind::try_from(kind_buf[0])?;
            info!("Received {} from {}", kind, addr.ip());

            match kind {
                SyncDataKind::File => {
                    self.handle_file(&mut stream).await?;
                }
                SyncDataKind::HandshakeRequest => {
                    self.handshake_service
                        .read_handshake(&mut stream, true)
                        .await?;
                }
                SyncDataKind::HandshakeResponse => {
                    self.handshake_service
                        .read_handshake(&mut stream, false)
                        .await?;
                }
            };
        }
    }

    async fn handle_file(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let src_ip = stream.peer_addr()?.ip();

        let recv_file = self.file_service.read_file(stream).await?;

        let synched_file = SynchedFile {
            name: recv_file.name.clone(),
            exists: true,
            hash: recv_file.hash.clone(),
            last_modified_at: recv_file.last_modified_at,
        };

        if let Ok(mut devices) = self.state.devices.write() {
            if let Some(device) = devices.get_mut(&src_ip) {
                device
                    .synched_files
                    .insert(synched_file.name.clone(), synched_file.clone());
            }
        }

        if let Ok(mut files) = self.state.synched_files.write() {
            files.insert(synched_file.name.clone(), synched_file);
        }

        // Save file
        let path = Path::new(&self.state.constants.files_dir).join(&recv_file.name);
        let mut file = File::create(path).await?;
        file.write_all(&recv_file.contents).await?;

        info!(
            "Successfully handled file: {} ({} bytes) from {}",
            recv_file.name, recv_file.size, src_ip
        );
        Ok(())
    }

    pub async fn sync_files(&self, mut sync_rx: Receiver<SynchedFile>) -> io::Result<()> {
        let mut buffer = HashMap::<String, SynchedFile>::new();
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                Some(file) = sync_rx.recv() => {
                    info!("File added to buffer: {}", file.name);
                    buffer.insert(file.name.clone(), file);
                }

                _ = interval.tick() => {
                    if buffer.is_empty() {
                        continue;
                    }

                    info!("Synching files: {:?}", buffer);

                    let devices = if let Ok(devices) = self.state.devices.read() {
                        devices
                            .values()
                            .filter(|device| {
                                buffer.values().any(|f| {
                                    device.synched_files
                                        .get(&f.name)
                                        .map(|device_file| device_file.hash != f.hash && device_file.last_modified_at < f.last_modified_at)
                                        .unwrap_or(false)
                                })
                            })
                            .cloned()
                            .collect::<Vec<_>>()
                    } else {
                        continue;
                    };

                    for device in devices {
                        for file in buffer.values() {
                            if device.synched_files.get(&file.name).map(|f| f.hash != file.hash && f.last_modified_at < file.last_modified_at).unwrap_or(false) {
                                if let Err(err) = self.file_service.send_file(file, device.addr).await {
                                    error!("Error synching file `{}`: {}", &file.name, err);
                                }
                            }
                        }
                    }

                    buffer.clear();
                }
            }
        }
    }
}
