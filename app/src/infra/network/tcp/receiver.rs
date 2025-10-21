use crate::{
    application::{
        AppState,
        network::transport::interface::{TransportError, TransportResult},
    },
    domain::{EntryInfo, EntryKind, TransportData},
    infra::network::tcp::kind::TcpStreamKind,
};
use sha2::{Digest, Sha256};
use std::{env, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub struct TcpReceiver {
    state: Arc<AppState>,
}

impl TcpReceiver {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn read_data(
        &self,
        mut stream: TcpStream,
        kind: TcpStreamKind,
    ) -> TransportResult<TransportData> {
        match kind {
            TcpStreamKind::HandshakeSyn => self.read_handshake(&mut stream, true).await,
            TcpStreamKind::HandshakeAck => self.read_handshake(&mut stream, false).await,
            TcpStreamKind::Metadata => self.read_metadata(&mut stream).await,
            TcpStreamKind::Request => self.read_request(&mut stream).await,
            TcpStreamKind::Transfer => self.read_transfer(&mut stream).await,
        }
    }

    async fn read_handshake(
        &self,
        stream: &mut TcpStream,
        is_syn: bool,
    ) -> TransportResult<TransportData> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        let data_str = String::from_utf8(buf).map_err(|e| TransportError::new(&e.to_string()))?;
        let data = serde_json::from_str(&data_str)?;

        if is_syn {
            Ok(TransportData::HandshakeSyn(data))
        } else {
            Ok(TransportData::HandshakeAck(data))
        }
    }

    async fn read_metadata(&self, stream: &mut TcpStream) -> TransportResult<TransportData> {
        let entry = self.read_entry_info(stream).await?;

        Ok(TransportData::Metadata(entry))
    }

    async fn read_request(&self, stream: &mut TcpStream) -> TransportResult<TransportData> {
        let entry = self.read_entry_info(stream).await?;

        Ok(TransportData::Request(entry))
    }

    async fn read_transfer(&self, stream: &mut TcpStream) -> TransportResult<TransportData> {
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
                return Err(TransportError::new(
                    "Hash mismatch: data corruption detected",
                ));
            }
        }

        self.save_entry(&entry, entry_buf).await?;

        Ok(TransportData::Transfer(entry))
    }

    async fn read_entry_info(&self, stream: &mut TcpStream) -> TransportResult<EntryInfo> {
        let mut json_len_buf = [0u8; 4];
        stream.read_exact(&mut json_len_buf).await?;
        let json_len = u32::from_be_bytes(json_len_buf) as usize;

        let mut json_buf = vec![0u8; json_len];
        stream.read_exact(&mut json_buf).await?;

        let entry = serde_json::from_slice::<EntryInfo>(&json_buf)?;
        Ok(entry)
    }

    async fn save_entry(&self, entry: &EntryInfo, contents: Vec<u8>) -> TransportResult<()> {
        let original_path = self.state.home_path.join(&*entry.name);
        let tmp_path = env::temp_dir().join(&*entry.name);

        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(&contents).await?;
        tmp_file.flush().await?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await?;
        Ok(())
    }
}
