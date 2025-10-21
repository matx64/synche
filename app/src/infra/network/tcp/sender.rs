use crate::{
    application::network::transport::interface::TransportResult,
    domain::{AppState, EntryInfo, HandshakeData, TransportData},
    infra::network::tcp::kind::TcpStreamKind,
};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::{info, warn};

pub struct TcpSender {
    state: Arc<AppState>,
}

impl TcpSender {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn send_data(&self, target: IpAddr, data: TransportData) -> TransportResult<()> {
        let kind = TcpStreamKind::from(&data);

        match data {
            TransportData::HandshakeSyn(hs_data) | TransportData::HandshakeAck(hs_data) => {
                self.send_handshake(target, hs_data, kind).await
            }
            TransportData::Metadata(entry) => self.send_metadata(target, entry).await,
            TransportData::Request(entry) => self.send_request(target, entry).await,
            TransportData::Transfer(entry) => self.send_entry(target, entry).await,
        }
    }

    async fn send_handshake(
        &self,
        target: IpAddr,
        hs_data: HandshakeData,
        kind: TcpStreamKind,
    ) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports.transport);
        let mut stream = TcpStream::connect(socket).await?;

        let contents = serde_json::to_vec(&hs_data)?;

        info!(kind = kind.to_string(), target = ?target, "[⬆️  SEND]");

        stream.write_all(self.state.local_id.as_bytes()).await?;
        stream.write_all(&[kind as u8]).await?;
        stream
            .write_all(&(contents.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(&contents).await?;

        Ok(())
    }

    async fn send_metadata(&self, target: IpAddr, entry: EntryInfo) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports.transport);
        let mut stream = TcpStream::connect(socket).await?;

        let kind = TcpStreamKind::Metadata;
        let contents = serde_json::to_vec(&entry)?;

        info!(kind = kind.to_string(), target = ?target, entry_name = ?&entry.name, "[⬆️  SEND]");

        stream.write_all(self.state.local_id.as_bytes()).await?;
        stream.write_all(&[kind as u8]).await?;
        stream
            .write_all(&u32::to_be_bytes(contents.len() as u32))
            .await?;
        stream.write_all(&contents).await?;
        Ok(())
    }

    async fn send_request(&self, target: IpAddr, entry: EntryInfo) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports.transport);
        let mut stream = TcpStream::connect(socket).await?;

        let kind = TcpStreamKind::Request;
        let contents = serde_json::to_vec(&entry)?;

        info!(kind = kind.to_string(), target = ?target, entry_name = ?&entry.name, "[⬆️  SEND]");

        stream.write_all(self.state.local_id.as_bytes()).await?;
        stream.write_all(&[kind as u8]).await?;
        stream
            .write_all(&u32::to_be_bytes(contents.len() as u32))
            .await?;
        stream.write_all(&contents).await?;
        Ok(())
    }

    async fn send_entry(&self, target: IpAddr, entry: EntryInfo) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports.transport);
        let mut stream = TcpStream::connect(socket).await?;

        let Some(contents) = self.read_entry_contents(&entry).await? else {
            return Ok(());
        };

        let kind = TcpStreamKind::Transfer;
        let metadata_json = serde_json::to_vec(&entry)?;
        let entry_size = contents.len() as u64;

        info!(kind = kind.to_string(), target = ?target, entry_name = ?&entry.name, "[⬆️  SEND]");

        // Write self peer id
        stream.write_all(self.state.local_id.as_bytes()).await?;

        // Write sync kind
        stream.write_all(&[kind as u8]).await?;

        // Write metadata json size
        stream
            .write_all(&u32::to_be_bytes(metadata_json.len() as u32))
            .await?;

        // Write metadata json
        stream.write_all(&metadata_json).await?;

        // Write entry size
        stream.write_all(&u64::to_be_bytes(entry_size)).await?;

        // Write entry contents
        stream.write_all(&contents).await?;
        Ok(())
    }

    async fn read_entry_contents(&self, entry: &EntryInfo) -> TransportResult<Option<Vec<u8>>> {
        let path = self.state.home_path.join(&*entry.name);

        let mut fs_file = File::open(path).await?;
        let mut buffer = Vec::new();
        fs_file.read_to_end(&mut buffer).await?;

        let hash = format!("{:x}", Sha256::digest(&buffer));
        if Some(hash) != entry.hash {
            warn!("⚠️  Cancelled File Transfer because it was modified during process.");
            return Ok(None);
        }

        Ok(Some(buffer))
    }
}
