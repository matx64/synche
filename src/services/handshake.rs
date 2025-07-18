use crate::{
    config::AppState,
    domain::{
        Peer,
        sync::{SyncHandshakeKind, SyncKind},
    },
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
        kind: SyncKind,
    ) -> io::Result<()> {
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        let contents = self.state.entry_manager.serialize()?;

        let content_b = contents.as_bytes();
        let content_len = content_b.len() as u32;

        info!("Sending {} to {}", kind, target_addr.ip());

        stream.write_all(&[kind.as_u8()]).await?;
        stream.write_all(&content_len.to_be_bytes()).await?;
        stream.write_all(content_b).await?;

        Ok(())
    }

    pub async fn read_handshake(
        &self,
        stream: &mut TcpStream,
        kind: SyncHandshakeKind,
    ) -> io::Result<()> {
        let src_addr = stream.peer_addr()?;

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len_buf = u32::from_be_bytes(len_buf) as usize;

        let mut content_buf = vec![0u8; len_buf];
        stream.read_exact(&mut content_buf).await?;
        let content = String::from_utf8(content_buf)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        let sync_data = self.state.entry_manager.deserialize(&content)?;

        self.state
            .peer_manager
            .insert(Peer::new(src_addr, Some(sync_data)));

        if matches!(kind, SyncHandshakeKind::Request) {
            self.send_handshake(src_addr, SyncKind::Handshake(SyncHandshakeKind::Response))
                .await?;
        }

        self.sync_peers(src_addr).await;

        Ok(())
    }

    async fn sync_peers(&self, target_addr: SocketAddr) {
        let ip = target_addr.ip();

        let Some(peer) = self.state.peer_manager.get(&ip) else {
            return;
        };

        info!("Synching peer: {}", ip);

        let files_to_send = self.state.entry_manager.get_files_to_send(&peer);

        for file in files_to_send {
            if let Err(err) = self.file_service.send_file(&file, target_addr).await {
                error!("Failed to send file {} to {}: {}", file.name, ip, err);
            }
        }
    }
}
