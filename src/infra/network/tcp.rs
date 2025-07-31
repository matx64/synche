use crate::{
    application::network::{
        TransportInterface,
        transport::interface::{TransportData, TransportStream},
    },
    domain::{EntryInfo, entry::entry::EntryKind},
    proto::transport::{PeerSyncData, SyncFileKind, SyncKind},
};
use sha2::{Digest, Sha256};
use std::{
    io::ErrorKind,
    net::{IpAddr, SocketAddr},
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::info;
use uuid::Uuid;

const TCP_PORT: u16 = 8889;

pub struct TcpTransporter {
    listener: TcpListener,
    local_id: Uuid,
}

impl TcpTransporter {
    pub async fn new(local_id: Uuid) -> Self {
        let listener = TcpListener::bind(format!("0.0.0.0:{TCP_PORT}"))
            .await
            .unwrap();

        Self { listener, local_id }
    }
}

impl TransportInterface for TcpTransporter {
    type Stream = TcpStream;

    async fn recv(&self) -> io::Result<TransportData<Self::Stream>> {
        let (mut stream, src_addr) = self.listener.accept().await?;
        let src_ip = src_addr.ip();

        let mut src_id_buf = [0u8; 16];
        stream.read_exact(&mut src_id_buf).await?;
        let src_id = Uuid::from_bytes(src_id_buf);

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;

        let kind = SyncKind::try_from(kind_buf[0])?;
        info!(kind = ?kind, id = ?src_id, ip = ?src_ip, "[🔔 RECV]");

        Ok(TransportData {
            src_id,
            src_ip,
            kind,
            stream,
        })
    }

    async fn send_handshake(
        &self,
        addr: IpAddr,
        kind: SyncKind,
        data: PeerSyncData,
    ) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let contents = serde_json::to_vec(&data)?;

        info!(kind = ?kind, target = ?addr, "[⬆️  SEND]");

        stream.write_all(self.local_id.as_bytes()).await?;
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

    async fn send_metadata(&self, addr: IpAddr, file: &EntryInfo) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let metadata_json = serde_json::to_vec(file)?;
        let kind = SyncKind::File(SyncFileKind::Metadata);

        info!(kind = ?kind, target = ?addr, file_name = ?&file.name, "[⬆️  SEND]");

        // Write self peer id
        stream.write_all(self.local_id.as_bytes()).await?;

        // Write sync kind
        stream.write_all(&[kind.as_u8()]).await?;

        // Write metadata json size
        stream
            .write_all(&u32::to_be_bytes(metadata_json.len() as u32))
            .await?;

        // Write metadata json
        stream.write_all(&metadata_json).await
    }

    async fn read_metadata(&self, stream: &mut TcpStream) -> io::Result<EntryInfo> {
        let mut json_len_buf = [0u8; 4];
        stream.read_exact(&mut json_len_buf).await?;
        let json_len = u32::from_be_bytes(json_len_buf) as usize;

        let mut json_buf = vec![0u8; json_len];
        stream.read_exact(&mut json_buf).await?;

        let metadata = serde_json::from_slice::<EntryInfo>(&json_buf)?;
        Ok(metadata)
    }

    async fn send_request(&self, addr: IpAddr, file: &EntryInfo) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let metadata_json = serde_json::to_vec(file)?;
        let kind = SyncKind::File(SyncFileKind::Request);

        info!(kind = ?kind, target = ?addr, file_name = ?&file.name, "[⬆️  SEND]");

        // Write self peer id
        stream.write_all(self.local_id.as_bytes()).await?;

        // Write sync kind
        stream.write_all(&[kind.as_u8()]).await?;

        // Write metadata json size
        stream
            .write_all(&u32::to_be_bytes(metadata_json.len() as u32))
            .await?;

        // Write metadata json
        stream.write_all(&metadata_json).await
    }

    async fn read_request(&self, stream: &mut TcpStream) -> io::Result<EntryInfo> {
        self.read_metadata(stream).await
    }

    async fn send_file(&self, addr: IpAddr, file: &EntryInfo, contents: &[u8]) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let metadata_json = serde_json::to_vec(file)?;
        let kind = SyncKind::File(SyncFileKind::Transfer);
        let file_size = contents.len() as u64;

        info!(kind = ?kind, target = ?addr, file_name = ?&file.name, "[⬆️  SEND]");

        // Write self peer id
        stream.write_all(self.local_id.as_bytes()).await?;

        // Write sync kind
        stream.write_all(&[kind.as_u8()]).await?;

        // Write metadata json size
        stream
            .write_all(&u32::to_be_bytes(metadata_json.len() as u32))
            .await?;

        // Write metadata json
        stream.write_all(&metadata_json).await?;

        // Write file size
        stream.write_all(&u64::to_be_bytes(file_size)).await?;

        // Write file contents
        stream.write_all(contents).await
    }

    async fn read_file(&self, stream: &mut TcpStream) -> io::Result<(EntryInfo, Vec<u8>)> {
        let metadata = self.read_metadata(stream).await?;

        let mut file_size_buf = [0u8; 8];
        stream.read_exact(&mut file_size_buf).await?;
        let file_size = u64::from_be_bytes(file_size_buf);

        let mut file_buf = vec![0u8; file_size as usize];
        stream.read_exact(&mut file_buf).await?;

        if let Some(hash) = &metadata.hash {
            if !metadata.is_deleted && matches!(metadata.kind, EntryKind::File) {
                let computed_hash = format!("{:x}", Sha256::digest(&file_buf));
                if computed_hash != *hash {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Hash mismatch: data corruption detected",
                    ));
                }
            }
        }

        Ok((metadata, file_buf))
    }
}

impl TransportStream for TcpStream {}
