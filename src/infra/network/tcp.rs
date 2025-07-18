use std::{io::ErrorKind, net::SocketAddr};

use crate::{
    application::network::TcpPort,
    domain::file::File,
    proto::tcp::{PeerSyncData, SyncKind},
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::info;

const TCP_PORT: u16 = 8889;

pub struct TcpTransporter {
    listener: TcpListener,
}

impl TcpTransporter {
    pub async fn new() -> Self {
        let listener = TcpListener::bind(format!("0.0.0.0:{TCP_PORT}"))
            .await
            .unwrap();

        Self { listener }
    }
}

impl TcpPort for TcpTransporter {
    async fn recv(&self) -> io::Result<(TcpStream, SyncKind)> {
        let (mut stream, src_addr) = self.listener.accept().await?;

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;

        let kind = SyncKind::try_from(kind_buf[0])?;
        info!("Received {} from {}", kind, src_addr.ip());

        Ok((stream, kind))
    }

    async fn send_handshake(
        &self,
        mut addr: SocketAddr,
        kind: SyncKind,
        data: PeerSyncData,
    ) -> io::Result<()> {
        addr.set_port(TCP_PORT);
        let mut stream = TcpStream::connect(addr).await?;

        info!("Sending {} to {}", kind, addr.ip());

        let contents = serde_json::to_vec(&data).map_err(|e| io::Error::other(e.to_string()))?;

        stream.write_all(&[kind.as_u8()]).await?;
        stream
            .write_all(&(contents.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(&contents).await
    }

    async fn read_handshake(&self, stream: &mut TcpStream) -> io::Result<PeerSyncData> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        let data = String::from_utf8(buf).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        serde_json::from_str(&data).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }

    async fn send_metadata(&self, addr: SocketAddr, file: &File) -> io::Result<()> {
        todo!()
    }

    async fn read_metadata(&self, stream: &mut TcpStream) -> io::Result<File> {
        todo!()
    }

    async fn send_request(&self, addr: SocketAddr, file: &File) -> io::Result<()> {
        todo!()
    }

    async fn read_request(&self, stream: &mut TcpStream) -> io::Result<File> {
        todo!()
    }

    async fn send_file(&self, addr: SocketAddr, file: &File) -> io::Result<()> {
        todo!()
    }

    async fn read_file(&self, stream: &mut TcpStream) -> io::Result<(File, Vec<u8>)> {
        todo!()
    }
}
