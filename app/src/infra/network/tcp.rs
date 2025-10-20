use crate::{
    application::network::{
        TransportInterface,
        transport::interface::{
            TransportData, TransportInterfaceV2, TransportRecvEvent, TransportResult,
            TransportStream,
        },
    },
    domain::{
        CanonicalPath, EntryInfo, EntryKind,
        transport::{HandshakeData, HandshakeKind, TransportDataV2},
    },
    proto::transport::{PeerHandshakeData, SyncEntryKind, SyncKind},
};
use sha2::{Digest, Sha256};
use std::{
    env,
    io::ErrorKind,
    net::{IpAddr, SocketAddr},
};
use tokio::{
    fs::{self, File},
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::info;
use uuid::Uuid;

const TCP_PORT: u16 = 8889;

pub struct TcpTransporter {
    local_id: Uuid,
    listener: TcpListener,
    home_path: CanonicalPath,
}

impl TcpTransporter {
    pub async fn new(local_id: Uuid, home_path: CanonicalPath) -> Self {
        let listener = TcpListener::bind(format!("0.0.0.0:{TCP_PORT}"))
            .await
            .unwrap();

        Self {
            local_id,
            listener,
            home_path,
        }
    }
}

impl TransportInterfaceV2 for TcpTransporter {
    async fn recv(&self) -> TransportResult<TransportRecvEvent> {
        let (mut stream, src_addr) = self.listener.accept().await?;
        let src_ip = src_addr.ip();

        let mut src_id_buf = [0u8; 16];
        stream.read_exact(&mut src_id_buf).await?;
        let src_id = Uuid::from_bytes(src_id_buf);

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;

        let data = match kind_buf[0] {
            1 => {
                self.read_handshake(&mut stream, HandshakeKind::Request)
                    .await?
            }

            2 => {
                self.read_handshake(&mut stream, HandshakeKind::Response)
                    .await?
            }

            3 => self.read_metadata(&mut stream).await?,
            4 => self.read_request(&mut stream).await?,
            5 => self.read_transfer(&mut stream).await?,

            _ => unreachable!(),
        };

        Ok(TransportRecvEvent {
            src_id,
            src_ip,
            data,
        })
    }

    async fn send(&self, target: IpAddr, data: TransportDataV2) -> TransportResult<()> {
        match data {
            TransportDataV2::Handshake(_) => todo!(),
            TransportDataV2::Metadata(_) => todo!(),
            TransportDataV2::Request(_) => todo!(),
            TransportDataV2::Transfer(_) => todo!(),
        }
    }
}

impl TcpTransporter {
    async fn read_handshake(
        &self,
        stream: &mut TcpStream,
        kind: HandshakeKind,
    ) -> io::Result<TransportDataV2> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        let data_str =
            String::from_utf8(buf).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
        let data = serde_json::from_str(&data_str)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        Ok(TransportDataV2::Handshake((data, kind)))
    }

    async fn read_metadata(&self, stream: &mut TcpStream) -> io::Result<TransportDataV2> {
        let entry = self.read_entry_info(stream).await?;

        Ok(TransportDataV2::Metadata(entry))
    }

    async fn read_request(&self, stream: &mut TcpStream) -> io::Result<TransportDataV2> {
        let entry = self.read_entry_info(stream).await?;

        Ok(TransportDataV2::Request(entry))
    }

    async fn read_transfer(&self, stream: &mut TcpStream) -> io::Result<TransportDataV2> {
        let entry = self.read_entry_info(stream).await?;

        let mut entry_size_buf = [0u8; 8];
        stream.read_exact(&mut entry_size_buf).await?;
        let entry_size = u64::from_be_bytes(entry_size_buf);

        let mut entry_buf = vec![0u8; entry_size as usize];
        stream.read_exact(&mut entry_buf).await?;

        if let Some(hash) = &entry.hash
            && !entry.is_removed()
            && matches!(entry.kind, EntryKind::File)
        {
            let computed_hash = format!("{:x}", Sha256::digest(&entry_buf));
            if computed_hash != *hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Hash mismatch: data corruption detected",
                ));
            }
        }

        self.save_entry(&entry, entry_buf).await?;

        Ok(TransportDataV2::Transfer(entry))
    }

    async fn read_entry_info(&self, stream: &mut TcpStream) -> io::Result<EntryInfo> {
        let mut json_len_buf = [0u8; 4];
        stream.read_exact(&mut json_len_buf).await?;
        let json_len = u32::from_be_bytes(json_len_buf) as usize;

        let mut json_buf = vec![0u8; json_len];
        stream.read_exact(&mut json_buf).await?;

        let entry = serde_json::from_slice::<EntryInfo>(&json_buf)?;
        Ok(entry)
    }

    async fn save_entry(&self, entry: &EntryInfo, contents: Vec<u8>) -> io::Result<()> {
        let original_path = self.home_path.join(&*entry.name);
        let tmp_path = env::temp_dir().join(&*entry.name);

        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(&contents).await?;
        tmp_file.flush().await?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await
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
        data: PeerHandshakeData,
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

    async fn read_handshake(&self, stream: &mut TcpStream) -> io::Result<PeerHandshakeData> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        let data = String::from_utf8(buf).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        serde_json::from_str(&data).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }

    async fn send_metadata(&self, addr: IpAddr, entry: &EntryInfo) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let metadata_json = serde_json::to_vec(entry)?;
        let kind = SyncKind::Entry(SyncEntryKind::Metadata);

        info!(kind = ?kind, target = ?addr, entry_name = ?&entry.name, "[⬆️  SEND]");

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

    async fn send_request(&self, addr: IpAddr, entry: &EntryInfo) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let metadata_json = serde_json::to_vec(entry)?;
        let kind = SyncKind::Entry(SyncEntryKind::Request);

        info!(kind = ?kind, target = ?addr, entry_name = ?&entry.name, "[⬆️  SEND]");

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

    async fn send_entry(&self, addr: IpAddr, entry: &EntryInfo, contents: &[u8]) -> io::Result<()> {
        let socket = SocketAddr::new(addr, TCP_PORT);
        let mut stream = TcpStream::connect(socket).await?;

        let metadata_json = serde_json::to_vec(entry)?;
        let kind = SyncKind::Entry(SyncEntryKind::Transfer);
        let entry_size = contents.len() as u64;

        info!(kind = ?kind, target = ?addr, entry_name = ?&entry.name, "[⬆️  SEND]");

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

        // Write entry size
        stream.write_all(&u64::to_be_bytes(entry_size)).await?;

        // Write entry contents
        stream.write_all(contents).await
    }

    async fn read_entry(&self, stream: &mut TcpStream) -> io::Result<(EntryInfo, Vec<u8>)> {
        let metadata = self.read_metadata(stream).await?;

        let mut entry_size_buf = [0u8; 8];
        stream.read_exact(&mut entry_size_buf).await?;
        let entry_size = u64::from_be_bytes(entry_size_buf);

        let mut entry_buf = vec![0u8; entry_size as usize];
        stream.read_exact(&mut entry_buf).await?;

        if let Some(hash) = &metadata.hash
            && !metadata.is_removed()
            && matches!(metadata.kind, EntryKind::File)
        {
            let computed_hash = format!("{:x}", Sha256::digest(&entry_buf));
            if computed_hash != *hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Hash mismatch: data corruption detected",
                ));
            }
        }

        Ok((metadata, entry_buf))
    }
}

impl TransportStream for TcpStream {}
