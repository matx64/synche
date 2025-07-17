use crate::{
    config::AppState,
    models::{
        entry::File,
        sync::{SyncFileKind, SyncKind},
    },
    services::{file::FileService, handshake::HandshakeService},
};
use std::sync::Arc;
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};
use tracing::info;

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

            let kind = SyncKind::try_from(kind_buf[0])?;
            info!("Received {} from {}", kind, addr.ip());

            match kind {
                SyncKind::Handshake(kind) => {
                    self.handshake_service
                        .read_handshake(&mut stream, kind)
                        .await?;
                }
                SyncKind::File(SyncFileKind::Metadata) => {
                    self.handle_file_metadata(&mut stream).await?;
                }
                SyncKind::File(SyncFileKind::Request) => {
                    self.handle_file_request(&mut stream).await?
                }
                SyncKind::File(SyncFileKind::Transfer) => {
                    self.handle_file_transfer(&mut stream).await?;
                }
            };
        }
    }

    async fn handle_file_metadata(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let src_addr = stream.peer_addr()?;
        let src_ip = src_addr.ip();

        let peer_file = self.file_service.read_metadata(stream).await?;

        let is_deleted = peer_file.is_deleted();
        if is_deleted {
            self.state
                .peer_manager
                .remove_file(&src_ip, &peer_file.name);
        } else {
            self.state
                .peer_manager
                .insert_file(&src_ip, peer_file.clone());
        }

        match self.state.entry_manager.get_file(&peer_file.name) {
            Some(local_file) => {
                if local_file.last_modified_at < peer_file.last_modified_at {
                    if is_deleted {
                        self.file_service.remove_file(&peer_file.name).await;
                        self.file_service
                            .send_metadata(&File::absent(peer_file.name), src_addr)
                            .await?;
                    } else if local_file.hash != peer_file.hash {
                        self.file_service
                            .send_request(&peer_file.name, src_addr)
                            .await?;
                    }
                }
            }

            None => {
                if !is_deleted {
                    self.file_service
                        .send_request(&peer_file.name, src_addr)
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_file_request(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let src_addr = stream.peer_addr()?;

        let file_name = self.file_service.read_request(stream).await?;

        if let Some(file) = self.state.entry_manager.get_file(&file_name) {
            self.file_service.send_file(&file, src_addr).await?;
        }
        Ok(())
    }

    async fn handle_file_transfer(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let src_addr = stream.peer_addr()?;

        let recv_file = self.file_service.read_file(stream).await?;
        let file = self.file_service.save_file(&recv_file).await?;

        self.file_service.send_metadata(&file, src_addr).await?;

        info!(
            "Successfully handled FileTransfer: {} ({} bytes) from {}",
            recv_file.name,
            recv_file.size,
            src_addr.ip()
        );
        Ok(())
    }
}
