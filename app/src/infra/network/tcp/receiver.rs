use crate::{
    application::AppState,
    application::network::transport::interface::{TransportError, TransportResult},
    domain::{EntryInfo, EntryKind, HandshakeData, RelativePath, SyncDirectory, TransportData},
    infra::network::tcp::{
        chunk::{MAX_TRANSFER_SIZE, TRANSFER_CHUNK_SIZE},
        kind::TcpStreamKind,
    },
    utils::fs::is_git_path,
};
use sha2::{Digest, Sha256};
use std::{env, path::PathBuf, sync::Arc};
use tokio::{
    fs::{self, File},
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use uuid::Uuid;

/// Inbound side of the TCP wire format.
///
/// Decodes a `TcpStreamKind`-tagged payload back into a
/// `TransportData`. For bulk transfers, the bytes are written to a
/// per-transfer staging file in the OS temp directory and only moved
/// to their final location after the streamed SHA-256 matches the
/// advertised hash and safety validation passes; corrupt or unsafe
/// transfers are dropped without touching the user's home tree.
pub struct TcpReceiver {
    state: Arc<AppState>,
}

struct Staging {
    root: PathBuf,
    tmp_path: PathBuf,
    file: File,
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
        let data = Self::validate_handshake_data(serde_json::from_str(&data_str)?)?;

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

        if entry_size > MAX_TRANSFER_SIZE {
            return Err(TransportError::new(&format!(
                "Transfer entry_size {entry_size} exceeds MAX_TRANSFER_SIZE {MAX_TRANSFER_SIZE}",
            )));
        }

        if is_git_path(&entry.name) {
            // Drain the payload from the wire without writing to disk.
            Self::discard_bytes(stream, entry_size, TRANSFER_CHUNK_SIZE).await?;
            return Ok(TransportData::Transfer(entry));
        }

        let mut staging = self.create_staging(&entry).await?;

        let computed_hash =
            match Self::stream_to_file(stream, &mut staging.file, entry_size, TRANSFER_CHUNK_SIZE)
                .await
            {
                Ok(h) => h,
                Err(e) => {
                    let _ = fs::remove_dir_all(&staging.root).await;
                    return Err(e);
                }
            };

        if let Some(hash) = &entry.hash
            && !entry.is_removed()
            && matches!(entry.kind, EntryKind::File)
            && computed_hash != *hash
        {
            let _ = fs::remove_dir_all(&staging.root).await;
            return Err(TransportError::new(
                "Hash mismatch: data corruption detected",
            ));
        }

        self.finalise_staging(&entry, staging).await?;

        Ok(TransportData::Transfer(entry))
    }

    async fn read_entry_info(&self, stream: &mut TcpStream) -> TransportResult<EntryInfo> {
        let mut json_len_buf = [0u8; 4];
        stream.read_exact(&mut json_len_buf).await?;
        let json_len = u32::from_be_bytes(json_len_buf) as usize;

        let mut json_buf = vec![0u8; json_len];
        stream.read_exact(&mut json_buf).await?;

        let entry = Self::validate_entry_info(serde_json::from_slice::<EntryInfo>(&json_buf)?)?;
        Ok(entry)
    }

    fn validate_handshake_data(data: HandshakeData) -> TransportResult<HandshakeData> {
        for SyncDirectory { name } in &data.sync_dirs {
            Self::validate_relative_path(name)?;
        }

        for (name, entry) in &data.entries {
            Self::validate_relative_path(name)?;
            Self::validate_relative_path(&entry.name)?;

            if name != &entry.name {
                return Err(TransportError::new(
                    "Handshake entry key does not match entry name",
                ));
            }
        }

        Ok(data)
    }

    fn validate_entry_info(entry: EntryInfo) -> TransportResult<EntryInfo> {
        Self::validate_relative_path(&entry.name)?;
        Ok(entry)
    }

    fn validate_relative_path(path: &RelativePath) -> TransportResult<()> {
        if path.is_safe_sync_path() {
            Ok(())
        } else {
            Err(TransportError::new("Unsafe sync path received"))
        }
    }

    async fn create_staging(&self, entry: &EntryInfo) -> TransportResult<Staging> {
        // Stage into a per-transfer subdirectory inside the OS temp dir.
        // Without the Uuid suffix, two concurrent transfers of the same
        // `entry.name` would race on the same `/tmp/<name>` staging file.
        let root = env::temp_dir().join(format!("synche-{}", Uuid::new_v4()));
        let tmp_path = root.join(&entry.name);

        if let Some(parent) = tmp_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let file = File::create(&tmp_path).await?;

        Ok(Staging {
            root,
            tmp_path,
            file,
        })
    }

    async fn finalise_staging(
        &self,
        entry: &EntryInfo,
        mut staging: Staging,
    ) -> TransportResult<()> {
        staging.file.flush().await?;
        drop(staging.file);

        let original_path = entry.name.to_canonical(self.state.home_path());
        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        match fs::rename(&staging.tmp_path, &original_path).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                fs::copy(&staging.tmp_path, &original_path).await?;
                fs::remove_file(&staging.tmp_path).await?;
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&staging.root).await;
                return Err(e.into());
            }
        }
        let _ = fs::remove_dir_all(&staging.root).await;
        Ok(())
    }

    /// Stream exactly `total` bytes from `reader` into `writer` in `chunk_size`
    /// chunks, returning the hex-encoded SHA-256 of the bytes streamed.
    pub(super) async fn stream_to_file<R, W>(
        reader: &mut R,
        writer: &mut W,
        total: u64,
        chunk_size: usize,
    ) -> TransportResult<String>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; chunk_size];
        let mut remaining = total;

        while remaining > 0 {
            let want = remaining.min(chunk_size as u64) as usize;
            reader.read_exact(&mut buf[..want]).await?;
            hasher.update(&buf[..want]);
            writer.write_all(&buf[..want]).await?;
            remaining -= want as u64;
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Read and discard exactly `total` bytes from `reader` in `chunk_size`
    /// chunks. Used when an incoming Transfer is filtered out (e.g. .git path)
    /// but the wire framing still requires consuming the advertised payload.
    pub(super) async fn discard_bytes<R>(
        reader: &mut R,
        total: u64,
        chunk_size: usize,
    ) -> TransportResult<()>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = vec![0u8; chunk_size];
        let mut remaining = total;
        while remaining > 0 {
            let want = remaining.min(chunk_size as u64) as usize;
            reader.read_exact(&mut buf[..want]).await?;
            remaining -= want as u64;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Cursor;
    use tokio::{
        fs,
        io::AsyncWriteExt,
        net::{TcpListener, TcpStream},
    };
    use uuid::Uuid;

    fn ok<T>(res: TransportResult<T>) -> T {
        match res {
            Ok(v) => v,
            Err(TransportError::Failure(m)) => panic!("{m}"),
        }
    }

    fn file_entry(name: &str, hash: Option<String>) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::File,
            hash,
            version: HashMap::from([(Uuid::new_v4(), 1)]),
        }
    }

    fn assert_transport_error<T>(res: TransportResult<T>, expected: &str) {
        match res {
            Err(TransportError::Failure(m)) => {
                assert!(m.contains(expected), "unexpected error: {m}")
            }
            Ok(_) => panic!("expected transport error containing {expected}"),
        }
    }

    #[test]
    fn validate_entry_info_rejects_paths_that_escape_home() {
        for path in [
            "/tmp/payload.bin",
            "../payload.bin",
            "sync/../../payload.bin",
        ] {
            assert_transport_error(
                TcpReceiver::validate_entry_info(file_entry(path, Some("hash".to_string()))),
                "Unsafe sync path",
            );
        }
    }

    #[test]
    fn validate_handshake_data_rejects_unsafe_remote_paths() {
        let entry = file_entry("../payload.bin", Some("hash".to_string()));
        let data = HandshakeData {
            hostname: "peer".to_string(),
            instance_id: Uuid::new_v4(),
            sync_dirs: vec![SyncDirectory {
                name: "sync".into(),
            }],
            entries: HashMap::from([(entry.name.clone(), entry)]),
        };

        assert_transport_error(
            TcpReceiver::validate_handshake_data(data),
            "Unsafe sync path",
        );
    }

    #[test]
    fn validate_handshake_data_rejects_mismatched_entry_keys() {
        let entry = file_entry("sync/payload.bin", Some("hash".to_string()));
        let data = HandshakeData {
            hostname: "peer".to_string(),
            instance_id: Uuid::new_v4(),
            sync_dirs: vec![SyncDirectory {
                name: "sync".into(),
            }],
            entries: HashMap::from([("sync/other.bin".into(), entry)]),
        };

        assert_transport_error(
            TcpReceiver::validate_handshake_data(data),
            "Handshake entry key does not match entry name",
        );
    }

    #[tokio::test]
    async fn stream_to_file_writes_exact_bytes_across_chunks() {
        let payload: Vec<u8> = (0..200u32).map(|i| (i % 256) as u8).collect();
        let mut src = Cursor::new(payload.clone());
        let mut dst: Vec<u8> = Vec::new();

        let hash =
            ok(TcpReceiver::stream_to_file(&mut src, &mut dst, payload.len() as u64, 16).await);

        assert_eq!(dst, payload);
        assert_eq!(hash, format!("{:x}", Sha256::digest(&payload)));
    }

    #[tokio::test]
    async fn stream_to_file_handles_exact_chunk_boundary() {
        let payload: Vec<u8> = (0..64u32).map(|i| (i % 256) as u8).collect();
        let mut src = Cursor::new(payload.clone());
        let mut dst: Vec<u8> = Vec::new();

        ok(TcpReceiver::stream_to_file(&mut src, &mut dst, payload.len() as u64, 16).await);

        assert_eq!(dst, payload);
    }

    #[tokio::test]
    async fn stream_to_file_handles_payload_smaller_than_chunk() {
        let payload = vec![0x55u8; 5];
        let mut src = Cursor::new(payload.clone());
        let mut dst: Vec<u8> = Vec::new();

        ok(TcpReceiver::stream_to_file(&mut src, &mut dst, payload.len() as u64, 1024).await);

        assert_eq!(dst, payload);
    }

    #[tokio::test]
    async fn discard_bytes_consumes_exact_count() {
        let payload = vec![0xFEu8; 100];
        let mut src = Cursor::new(payload);

        ok(TcpReceiver::discard_bytes(&mut src, 100, 16).await);

        // After discarding everything, the cursor should be at EOF.
        let mut tail = Vec::new();
        src.read_to_end(&mut tail).await.unwrap();
        assert!(tail.is_empty());
    }

    async fn write_transfer_to_stream(stream: &mut TcpStream, entry: &EntryInfo, contents: &[u8]) {
        let entry_json = serde_json::to_vec(entry).unwrap();
        stream
            .write_all(&(entry_json.len() as u32).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(&entry_json).await.unwrap();
        stream
            .write_all(&(contents.len() as u64).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(contents).await.unwrap();
    }

    #[tokio::test]
    async fn read_transfer_streams_file_to_home_and_validates_hash() {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let root = format!("tcp_chunk_ok_{}", Uuid::new_v4());
        let entry_name = format!("{root}/payload.bin");
        let original_path = state.home_path().join(&entry_name);
        let contents: Vec<u8> = (0..200u32).map(|i| (i % 256) as u8).collect();
        let hash = format!("{:x}", Sha256::digest(&contents));
        let entry = file_entry(&entry_name, Some(hash));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let entry_clone = entry.clone();
        let contents_clone = contents.clone();
        let writer = tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            write_transfer_to_stream(&mut stream, &entry_clone, &contents_clone).await;
        });

        let (stream, _) = listener.accept().await.unwrap();
        let receiver = TcpReceiver::new(state.clone());
        let data = ok(receiver.read_data(stream, TcpStreamKind::Transfer).await);
        writer.await.unwrap();

        assert!(matches!(data, TransportData::Transfer(_)));
        assert!(original_path.exists());
        let on_disk = fs::read(&original_path).await.unwrap();
        assert_eq!(on_disk, contents);

        let root_path = state.home_path().join(&root);
        fs::remove_dir_all(root_path).await.unwrap();
    }

    #[tokio::test]
    async fn read_transfer_rejects_hash_mismatch_and_cleans_staging() {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let root = format!("tcp_chunk_bad_{}", Uuid::new_v4());
        let entry_name = format!("{root}/payload.bin");
        let original_path = state.home_path().join(&entry_name);
        let contents = vec![0xAAu8; 64];
        let entry = file_entry(&entry_name, Some("deadbeef".to_string()));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let entry_clone = entry.clone();
        let contents_clone = contents.clone();
        let writer = tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            write_transfer_to_stream(&mut stream, &entry_clone, &contents_clone).await;
        });

        let (stream, _) = listener.accept().await.unwrap();
        let receiver = TcpReceiver::new(state.clone());
        let result = receiver.read_data(stream, TcpStreamKind::Transfer).await;
        writer.await.unwrap();

        match result {
            Err(TransportError::Failure(m)) => {
                assert!(m.contains("Hash mismatch"), "unexpected error: {m}")
            }
            Ok(_) => panic!("expected hash-mismatch failure"),
        }
        assert!(!original_path.exists());

        let root_path = state.home_path().join(&root);
        if root_path.exists() {
            fs::remove_dir_all(root_path).await.unwrap();
        }
    }

    #[tokio::test]
    async fn read_transfer_rejects_oversized_payload() {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let root = format!("tcp_oversize_{}", Uuid::new_v4());
        let entry_name = format!("{root}/payload.bin");
        let original_path = state.home_path().join(&entry_name);
        let entry = file_entry(&entry_name, Some("ignored".to_string()));
        let entry_json = serde_json::to_vec(&entry).unwrap();
        let oversized = crate::infra::network::tcp::chunk::MAX_TRANSFER_SIZE + 1;

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
                .write_all(&u64::to_be_bytes(oversized))
                .await
                .unwrap();
        });

        let (stream, _) = listener.accept().await.unwrap();
        let receiver = TcpReceiver::new(state.clone());
        let result = receiver.read_data(stream, TcpStreamKind::Transfer).await;
        writer.await.unwrap();

        match result {
            Err(TransportError::Failure(m)) => {
                assert!(m.contains("MAX_TRANSFER_SIZE"), "unexpected error: {m}")
            }
            Ok(_) => panic!("expected oversize rejection"),
        }
        assert!(!original_path.exists());
        let root_path = state.home_path().join(&root);
        assert!(!root_path.exists());
    }

    #[tokio::test]
    async fn read_transfer_consumes_git_entry_without_writing_to_home() {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let root = format!("tcp_git_guard_{}", Uuid::new_v4());
        let entry_name = format!("{root}/.git/config");
        let original_path = state.home_path().join(&entry_name);
        let entry = file_entry(&entry_name, Some("intentionally-invalid-hash".to_string()));
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
        let data = ok(receiver.read_data(stream, TcpStreamKind::Transfer).await);
        writer.await.unwrap();

        assert!(matches!(data, TransportData::Transfer(_)));
        assert!(!original_path.exists());

        let root_path = state.home_path().join(&root);
        if root_path.exists() {
            fs::remove_dir_all(root_path).await.unwrap();
        }
    }
}
