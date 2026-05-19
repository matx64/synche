use crate::{
    application::AppState,
    application::network::transport::interface::{TransportError, TransportResult},
    domain::{EntryInfo, EntryKind, TransportData},
    infra::network::tcp::kind::TcpStreamKind,
    utils::fs::is_git_path,
};
use sha2::{Digest, Sha256};
use std::{env, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use uuid::Uuid;

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

        if is_git_path(&entry.name) {
            return Ok(TransportData::Transfer(entry));
        }

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
        let original_path = entry.name.to_canonical(self.state.home_path());
        // Stage into a per-transfer subdirectory inside the OS temp dir.
        // Without the Uuid suffix, two concurrent transfers of the same
        // `entry.name` would race on the same `/tmp/<name>` staging file.
        let staging_root = env::temp_dir().join(format!("synche-{}", Uuid::new_v4()));
        let tmp_path = staging_root.join(&entry.name);

        if let Some(parent) = tmp_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(&contents).await?;
        tmp_file.flush().await?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        match fs::rename(&tmp_path, &original_path).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                fs::copy(&tmp_path, &original_path).await?;
                fs::remove_file(&tmp_path).await?;
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&staging_root).await;
                return Err(e.into());
            }
        }
        let _ = fs::remove_dir_all(&staging_root).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::{
        fs,
        io::AsyncWriteExt,
        net::{TcpListener, TcpStream},
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn read_transfer_consumes_git_entry_without_writing_to_home() {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let root = format!("tcp_git_guard_{}", Uuid::new_v4());
        let entry_name = format!("{root}/.git/config");
        let original_path = state.home_path().join(&entry_name);
        let entry = EntryInfo {
            name: entry_name.clone().into(),
            kind: EntryKind::File,
            hash: Some("intentionally-invalid-hash".to_string()),
            version: HashMap::from([(Uuid::new_v4(), 1)]),
        };
        let contents = b"[core]\nrepositoryformatversion = 0\n".to_vec();
        let entry_json = serde_json::to_vec(&entry).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let writer = tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(&(entry_json.len() as u32).to_be_bytes())
                .await
                .unwrap();
            stream.write_all(&entry_json).await.unwrap();
            stream
                .write_all(&(contents.len() as u64).to_be_bytes())
                .await
                .unwrap();
            stream.write_all(&contents).await.unwrap();
        });

        let (stream, _) = listener.accept().await.unwrap();
        let receiver = TcpReceiver::new(state.clone());
        let data = match receiver.read_data(stream, TcpStreamKind::Transfer).await {
            Ok(data) => data,
            Err(TransportError::Failure(message)) => panic!("{message}"),
        };
        writer.await.unwrap();

        assert!(matches!(data, TransportData::Transfer(_)));
        assert!(!original_path.exists());

        let root_path = state.home_path().join(&root);
        if root_path.exists() {
            fs::remove_dir_all(root_path).await.unwrap();
        }
    }
}
