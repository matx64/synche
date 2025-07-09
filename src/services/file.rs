use crate::{
    config::AppState,
    models::{
        entry::Entry,
        sync::{ReceivedFile, SyncDataKind},
    },
};
use filetime::FileTime;
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::{
    fs::{self, File},
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

pub struct FileService {
    state: Arc<AppState>,
}

impl FileService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn send_file(
        &self,
        entry: &Entry,
        mut target_addr: SocketAddr,
    ) -> std::io::Result<()> {
        let target_ip = target_addr.ip();

        let path = self.state.constants.entries_dir.join(&entry.name);
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();

        info!("Sending file {} to {}", file_name, target_ip);

        // Read file content
        let mut file = File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        // Get last modified at
        let last_modified_at = entry
            .last_modified_at
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Connect to target peer's TCP server
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        // Send data kind
        stream.write_all(&[SyncDataKind::File as u8]).await?;

        // Send file name length (u64)
        stream
            .write_all(&u64::to_be_bytes(file_name.len() as u64))
            .await?;

        // Send file name
        stream.write_all(file_name.as_bytes()).await?;

        // Send file hash (32 bytes)
        let hash_bytes = hex::decode(&entry.hash).unwrap();
        stream.write_all(&hash_bytes).await?;

        // Send file size (u64)
        let file_size = buffer.len() as u64;
        stream.write_all(&u64::to_be_bytes(file_size)).await?;

        // Send file contents
        stream.write_all(&buffer).await?;

        // Send last modification date
        stream.write_all(&last_modified_at.to_be_bytes()).await?;

        self.state
            .peer_manager
            .insert_entry(&target_ip, entry.to_owned());

        info!(
            "Sent file: {} ({} bytes) to {}",
            file_name, file_size, target_ip
        );
        Ok(())
    }

    pub async fn read_file(&self, socket: &mut TcpStream) -> std::io::Result<ReceivedFile> {
        // Read file name length (u64)
        let mut name_len_buf = [0u8; 8];
        socket.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        // Read file name
        let mut file_name_buf = vec![0u8; name_len];
        socket.read_exact(&mut file_name_buf).await?;
        let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

        // Read file hash (32 bytes)
        let mut hash_buf = [0u8; 32];
        socket.read_exact(&mut hash_buf).await?;
        let received_hash = hex::encode(hash_buf);

        // Read file size (u64)
        let mut file_size_buf = [0u8; 8];
        socket.read_exact(&mut file_size_buf).await?;
        let file_size = u64::from_be_bytes(file_size_buf);

        // Read file contents
        let mut file_buf = vec![0u8; file_size as usize];
        socket.read_exact(&mut file_buf).await?;

        // Compute hash for corruption check
        let computed_hash = format!("{:x}", Sha256::digest(&file_buf));
        if computed_hash != received_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Hash mismatch: data corruption detected",
            ));
        }

        // Read file last modification date
        let mut timestamp_buf = [0u8; 8];
        socket.read_exact(&mut timestamp_buf).await?;

        let last_modified_at = UNIX_EPOCH + Duration::from_secs(u64::from_be_bytes(timestamp_buf));

        Ok(ReceivedFile {
            name: file_name,
            size: file_size,
            contents: file_buf,
            hash: received_hash,
            last_modified_at,
        })
    }

    pub async fn save_file(&self, file: &ReceivedFile) -> io::Result<()> {
        let original_path = self.state.constants.entries_dir.join(&file.name);
        let tmp_path = self.state.constants.tmp_dir.join(&file.name);

        if let Some(parent) = tmp_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(&file.contents).await?;
        tmp_file.flush().await?;

        let mtime = FileTime::from_system_time(file.last_modified_at);
        filetime::set_file_mtime(&tmp_path, mtime)?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await
    }
}
