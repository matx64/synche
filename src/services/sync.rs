use crate::{
    config::AppState,
    models::{entry::File, sync::SyncDataKind},
    services::{file::FileService, handshake::HandshakeService},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    io::{self, AsyncReadExt},
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

        if !recv_file.hash.is_empty() {
            self.file_service.save_file(&src_ip, &recv_file).await?;
        } else {
            self.file_service.remove_file(&src_ip, &recv_file).await?;
        }

        info!(
            "Successfully handled file: {} ({} bytes) from {}",
            recv_file.name, recv_file.size, src_ip
        );
        Ok(())
    }

    pub async fn sync_files(&self, mut sync_rx: Receiver<File>) -> io::Result<()> {
        let mut buffer = HashMap::<String, File>::new();
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

                    let sync_map = self.state.peer_manager.build_sync_map(&buffer);

                    for (addr, files) in sync_map {
                        for file in files {
                            if let Err(err) = self.file_service.send_file(file, addr).await {
                                error!("Error synching file `{}`: {}", &file.name, err);
                            }
                        }
                    }

                    buffer.clear();
                }
            }
        }
    }
}
