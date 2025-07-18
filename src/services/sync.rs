use crate::{
    config::AppState,
    models::sync::{SyncFileKind, SyncKind},
    services::{file::FileService, handshake::HandshakeService},
};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};
use tracing::{info, warn};

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
            let (mut stream, src_addr) = listener.accept().await?;

            let mut kind_buf = [0u8; 1];
            stream.read_exact(&mut kind_buf).await?;

            let kind = SyncKind::try_from(kind_buf[0])?;
            info!("Received {} from {}", kind, src_addr.ip());

            // TODO: Async
            match kind {
                SyncKind::Handshake(kind) => {
                    self.handshake_service
                        .read_handshake(&mut stream, kind)
                        .await?;
                }
                SyncKind::File(SyncFileKind::Metadata) => {
                    self.handle_file_metadata(src_addr, &mut stream).await?;
                }
                SyncKind::File(SyncFileKind::Request) => {
                    self.handle_file_request(src_addr, &mut stream).await?
                }
                SyncKind::File(SyncFileKind::Transfer) => {
                    self.handle_file_transfer(src_addr, &mut stream).await?;
                }
            };
        }
    }

    async fn handle_file_metadata(
        &self,
        src_addr: SocketAddr,
        stream: &mut TcpStream,
    ) -> std::io::Result<()> {
        let src_ip = src_addr.ip();

        let peer_file = self.file_service.read_metadata(src_ip, stream).await?;

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
                if local_file.hash != peer_file.hash {
                    if local_file.version < peer_file.version {
                        if is_deleted {
                            self.file_service
                                .remove_file(src_addr, &peer_file.name)
                                .await?;
                        } else {
                            self.file_service.send_request(&peer_file, src_addr).await?;
                        }
                    } else if local_file.version == peer_file.version {
                        // TODO: Handle Conflict
                        warn!("FILE VERSION CONFLICT: {}", local_file.name);
                    }
                }
            }

            None => {
                if !is_deleted {
                    self.file_service.send_request(&peer_file, src_addr).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_file_request(
        &self,
        src_addr: SocketAddr,
        stream: &mut TcpStream,
    ) -> std::io::Result<()> {
        let req_file = self.file_service.read_request(stream).await?;

        if let Some(file) = self.state.entry_manager.get_file(&req_file.name) {
            if file.hash == req_file.hash && file.version == req_file.version {
                self.file_service.send_file(&file, src_addr).await?;
            }
        }
        Ok(())
    }

    async fn handle_file_transfer(
        &self,
        src_addr: SocketAddr,
        stream: &mut TcpStream,
    ) -> std::io::Result<()> {
        let src_ip = src_addr.ip();

        let recv_file = self.file_service.read_file(src_ip, stream).await?;
        self.file_service.save_file(src_addr, &recv_file).await?;

        info!(
            "Successfully handled FileTransfer: {} ({} bytes) from {}",
            recv_file.name, recv_file.size, src_ip
        );
        Ok(())
    }
}
