use crate::{
    application::network::TransportInterface,
    domain::FileInfo,
    proto::tcp::{PeerSyncData, SyncFileKind, SyncKind},
};
use sha2::{Digest, Sha256};
use std::{io::ErrorKind, net::SocketAddr};
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

impl TransportInterface for TcpTransporter {
    type Stream = TcpStream;

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

    async fn send_metadata(&self, mut addr: SocketAddr, file: &FileInfo) -> io::Result<()> {
        addr.set_port(TCP_PORT);
        let mut stream = TcpStream::connect(addr).await?;

        info!("Sending Metadata [{}] to {}", &file.name, addr.ip());

        stream
            .write_all(&[SyncKind::File(SyncFileKind::Metadata).as_u8()])
            .await?;

        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        stream.write_all(file.name.as_bytes()).await?;

        let hash_bytes = hex::decode(&file.hash).map_err(io::Error::other)?;
        stream.write_all(&hash_bytes).await?;

        stream.write_all(&u32::to_be_bytes(file.version)).await
    }

    async fn read_metadata(&self, stream: &mut TcpStream) -> io::Result<FileInfo> {
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        let mut name_buf = vec![0u8; name_len];
        stream.read_exact(&mut name_buf).await?;
        let name = String::from_utf8_lossy(&name_buf).into_owned();

        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let hash = hex::encode(hash_buf);

        let mut version_buf = [0u8; 4];
        stream.read_exact(&mut version_buf).await?;
        let version = u32::from_be_bytes(version_buf);

        Ok(FileInfo {
            name,
            hash,
            version,
            last_modified_by: Some(stream.peer_addr()?.ip()),
        })
    }

    async fn send_request(&self, mut addr: SocketAddr, file: &FileInfo) -> io::Result<()> {
        addr.set_port(TCP_PORT);
        let mut stream = TcpStream::connect(addr).await?;

        info!("Sending Request [{}] to {}", &file.name, addr.ip());

        stream
            .write_all(&[SyncKind::File(SyncFileKind::Request).as_u8()])
            .await?;

        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        stream.write_all(file.name.as_bytes()).await?;

        let hash_bytes = hex::decode(&file.hash).map_err(io::Error::other)?;
        stream.write_all(&hash_bytes).await?;

        stream.write_all(&u32::to_be_bytes(file.version)).await
    }

    async fn read_request(&self, stream: &mut TcpStream) -> io::Result<FileInfo> {
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        let mut name_buf = vec![0u8; name_len];
        stream.read_exact(&mut name_buf).await?;
        let name = String::from_utf8_lossy(&name_buf).into_owned();

        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let hash = hex::encode(hash_buf);

        let mut version_buf = [0u8; 4];
        stream.read_exact(&mut version_buf).await?;
        let version = u32::from_be_bytes(version_buf);

        Ok(FileInfo {
            name,
            hash,
            version,
            last_modified_by: None,
        })
    }

    async fn send_file(
        &self,
        mut addr: SocketAddr,
        file: &FileInfo,
        contents: &[u8],
    ) -> io::Result<()> {
        addr.set_port(TCP_PORT);
        let mut stream = TcpStream::connect(addr).await?;

        info!("Sending File [{}] to {}", &file.name, addr.ip());

        stream
            .write_all(&[SyncKind::File(SyncFileKind::Transfer).as_u8()])
            .await?;

        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        stream.write_all(file.name.as_bytes()).await?;

        let hash_bytes = hex::decode(&file.hash).map_err(io::Error::other)?;
        stream.write_all(&hash_bytes).await?;

        stream.write_all(&u32::to_be_bytes(file.version)).await?;

        let file_size = contents.len() as u64;
        stream.write_all(&u64::to_be_bytes(file_size)).await?;

        stream.write_all(contents).await
    }

    async fn read_file(&self, stream: &mut TcpStream) -> io::Result<(FileInfo, Vec<u8>)> {
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        let mut name_buf = vec![0u8; name_len];
        stream.read_exact(&mut name_buf).await?;
        let name = String::from_utf8_lossy(&name_buf).into_owned();

        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let hash = hex::encode(hash_buf);

        let mut version_buf = [0u8; 4];
        stream.read_exact(&mut version_buf).await?;
        let version = u32::from_be_bytes(version_buf);

        let mut file_size_buf = [0u8; 8];
        stream.read_exact(&mut file_size_buf).await?;
        let file_size = u64::from_be_bytes(file_size_buf);

        let mut file_buf = vec![0u8; file_size as usize];
        stream.read_exact(&mut file_buf).await?;

        // Compute hash for corruption check
        let computed_hash = format!("{:x}", Sha256::digest(&file_buf));
        if computed_hash != hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Hash mismatch: data corruption detected",
            ));
        }

        Ok((
            FileInfo {
                name,
                hash,
                version,
                last_modified_by: Some(stream.peer_addr()?.ip()),
            },
            file_buf,
        ))
    }
}
