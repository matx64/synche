use crate::{
    config::AppState,
    models::{
        entry::File,
        sync::{ReceivedFile, SyncDataKind},
    },
};
use filetime::FileTime;
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::{
    fs::{self, File as FsFile},
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

    pub async fn send_file(&self, file: &File, mut target_addr: SocketAddr) -> std::io::Result<()> {
        let target_ip = target_addr.ip();

        let path = self.state.constants.base_dir.join(&file.name);

        info!("Sending file {} to {}", file.name, target_ip);

        // Read file content
        let mut fs_file = FsFile::open(path).await?;
        let mut buffer = Vec::new();
        fs_file.read_to_end(&mut buffer).await?;

        // Get last modified at
        let last_modified_at = file
            .last_modified_at
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Connect to target peer's TCP server
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        // Send data kind
        stream
            .write_all(&[SyncDataKind::FileTransfer as u8])
            .await?;

        // Send file name length (u64)
        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        // Send file name
        stream.write_all(file.name.as_bytes()).await?;

        // Send file hash (32 bytes)
        let hash_bytes = hex::decode(&file.hash.clone()).unwrap();
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
            .insert_file(&target_ip, file.to_owned());

        info!(
            "Sent file: {} ({} bytes) to {}",
            file.name, file_size, target_ip
        );
        Ok(())
    }

    pub async fn read_file(&self, stream: &mut TcpStream) -> std::io::Result<ReceivedFile> {
        // Read file name length (u64)
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        // Read file name
        let mut file_name_buf = vec![0u8; name_len];
        stream.read_exact(&mut file_name_buf).await?;
        let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

        // Read file hash (32 bytes)
        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let received_hash = hex::encode(hash_buf);

        // Read file size (u64)
        let mut file_size_buf = [0u8; 8];
        stream.read_exact(&mut file_size_buf).await?;
        let file_size = u64::from_be_bytes(file_size_buf);

        // Read file contents
        let mut file_buf = vec![0u8; file_size as usize];
        stream.read_exact(&mut file_buf).await?;

        // Compute hash for corruption check
        if !received_hash.is_empty() {
            let computed_hash = format!("{:x}", Sha256::digest(&file_buf));
            if computed_hash != received_hash {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Hash mismatch: data corruption detected",
                ));
            }
        }

        // Read file last modification date
        let mut timestamp_buf = [0u8; 8];
        stream.read_exact(&mut timestamp_buf).await?;

        let last_modified_at = UNIX_EPOCH + Duration::from_secs(u64::from_be_bytes(timestamp_buf));

        Ok(ReceivedFile {
            name: file_name,
            size: file_size,
            contents: file_buf,
            hash: received_hash,
            last_modified_at,
        })
    }

    pub async fn send_file_removed(
        &self,
        file: &File,
        mut target_addr: SocketAddr,
    ) -> std::io::Result<()> {
        let target_ip = target_addr.ip();

        // Connect to target peer's TCP server
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        // Send data kind
        stream.write_all(&[SyncDataKind::FileRemoved as u8]).await?;

        // Send file name length (u64)
        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        // Send file name
        stream.write_all(file.name.as_bytes()).await?;

        self.state.peer_manager.remove_file(&target_ip, &file.name);

        Ok(())
    }

    pub async fn read_file_removed(&self, stream: &mut TcpStream) -> std::io::Result<String> {
        // Read file name length (u64)
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        // Read file name
        let mut file_name_buf = vec![0u8; name_len];
        stream.read_exact(&mut file_name_buf).await?;
        let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

        Ok(file_name)
    }

    pub async fn save_file(&self, src_ip: &IpAddr, recv_file: &ReceivedFile) -> io::Result<()> {
        let file = File {
            name: recv_file.name.clone(),
            hash: recv_file.hash.clone(),
            last_modified_at: recv_file.last_modified_at,
        };

        self.state.peer_manager.insert_file(src_ip, file.clone());
        self.state.entry_manager.insert_file(file);

        let original_path = self.state.constants.base_dir.join(&recv_file.name);
        let tmp_path = self.state.constants.tmp_dir.join(&recv_file.name);

        if let Some(parent) = tmp_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut tmp_file = FsFile::create(&tmp_path).await?;
        tmp_file.write_all(&recv_file.contents).await?;
        tmp_file.flush().await?;

        let mtime = FileTime::from_system_time(recv_file.last_modified_at);
        filetime::set_file_mtime(&tmp_path, mtime)?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await
    }

    pub async fn remove_file(&self, src_ip: &IpAddr, filename: &str) -> io::Result<()> {
        self.state.entry_manager.remove_file(filename);
        self.state.peer_manager.remove_file(src_ip, filename);

        let path = self.state.constants.base_dir.join(filename);
        fs::remove_file(path).await
    }
}
